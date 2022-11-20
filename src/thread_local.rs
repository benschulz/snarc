//! Defines a generic `Snarc` based on the `thread_local` crate.
//!
//! The `snarc!` macro declares a single `thread_local!` that is shared by all
//! instances of its defined structs. That means that two of `snarc!`'s
//! instances cannot bind their value to the current thread at the same time.
//! The `Snarc` defined in this module uses a per-instance `ThreadLocal` to lift
//! this restriction.
use std::alloc;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ptr;

use crate::Context;
use crate::ErasedNarc;
use crate::ErasedSnarc;
use crate::State;

struct SnarcBox<T> {
    count: std::cell::Cell<usize>,
    thread_local: thread_local::ThreadLocal<std::cell::Cell<State>>,
    value: T,
}

impl<T> SnarcBox<T> {
    fn new_ptr(value: T) -> *mut Self {
        Box::leak(Box::new(Self {
            count: std::cell::Cell::new(0),
            thread_local: thread_local::ThreadLocal::new(),
            value,
        }))
    }

    #[inline]
    #[allow(clippy::mut_from_ref)]
    unsafe fn into_mut_unchecked(ptr: &*mut SnarcBox<T>) -> &mut T {
        &mut (**ptr).value
    }
}

/// A sendable, owning reference-counted pointer to a `T`.
pub struct Snarc<T> {
    ptr: *mut SnarcBox<T>,
    phantom: std::marker::PhantomData<SnarcBox<T>>,
}

unsafe impl<T: Send> Send for Snarc<T> {}
unsafe impl<T: Sync> Sync for Snarc<T> {}

impl<T> Snarc<T> {
    /// Creates a new `Snarc` with the given inner `value`.
    pub fn new(value: T) -> Self {
        Self {
            ptr: SnarcBox::new_ptr(value),
            phantom: std::marker::PhantomData,
        }
    }

    /// Turn this `Snarc` into the `!Send` version `Narc`.
    pub fn into_unsend(mut self) -> Narc<T> {
        let narc = Narc {
            ptr: self.ptr,
            phantom: self.phantom,
        };

        self.ptr = ptr::null_mut();

        narc
    }

    /// Turn this parameterized `Snarc` the unparameterized `ErasedSnarc`.
    pub fn into_erased(self) -> ErasedSnarc
    where
        T: Send + 'static,
    {
        ErasedSnarc {
            inner: Box::new(self),
        }
    }

    #[inline(always)]
    fn inner(&self) -> &SnarcBox<T> {
        unsafe { &*self.ptr }
    }

    /// Creates a new non-owning reference to the inner value.
    pub fn new_ref(&self) -> SnarcRef<T> {
        let inner = self.inner();

        inner.count.set(inner.count.get() + 1);

        SnarcRef {
            ptr: self.ptr,
            phantom: Default::default(),
        }
    }

    /// Temporarily bind the inner value to this thread and evaluate `f` within
    /// that context.
    pub fn enter<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        let inner = self.inner();
        inner.thread_local.get_or_default().set(State::Entered);

        let _guard = scopeguard::guard((), |_| {
            inner.thread_local.get_or_default().set(State::Default);
        });

        f(&inner.value)
    }
}

impl<T: Send + 'static> From<Snarc<T>> for ErasedSnarc {
    fn from(snarc: Snarc<T>) -> Self {
        snarc.into_erased()
    }
}

impl<T: Send + 'static> From<Snarc<T>> for ErasedNarc {
    fn from(snarc: Snarc<T>) -> Self {
        snarc.into_unsend().into_erased()
    }
}

impl<T> Context for Snarc<T> {
    fn set(&mut self, v: State) {
        self.inner().thread_local.get_or_default().set(v)
    }
}

impl<T> Deref for Snarc<T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.inner().value
    }
}

impl<T> DerefMut for Snarc<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { SnarcBox::into_mut_unchecked(&self.ptr) }
    }
}

impl<T> Drop for Snarc<T> {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            {
                self.inner()
                    .thread_local
                    .get_or_default()
                    .set(State::Entered);

                let _guard = scopeguard::guard((), |_| {
                    self.inner()
                        .thread_local
                        .get_or_default()
                        .set(State::Default)
                });

                unsafe {
                    // destroy the contained object
                    ptr::drop_in_place(SnarcBox::into_mut_unchecked(&self.ptr));
                }
            }

            if self.inner().count.get() == 0 {
                unsafe {
                    ptr::addr_of_mut!((*self.ptr).count).drop_in_place();
                    let layout = alloc::Layout::for_value(&*self.ptr);
                    alloc::dealloc(self.ptr.cast(), layout);
                }
            }
        }
    }
}

/// A non-sendable, owning reference-counted pointer to a `T`.
///
/// When `Narc` is used exclusively, i.e., never `Snarc`, then `Rc` should
/// likely be used instead.
pub struct Narc<T> {
    ptr: *mut SnarcBox<T>,
    phantom: std::marker::PhantomData<SnarcBox<T>>,
}

unsafe impl<T: Sync> Sync for Narc<T> {}

impl<T> Narc<T> {
    /// Creates a new `Narc` with the given inner `value`.
    pub fn new(value: T) -> Self {
        Self {
            ptr: SnarcBox::new_ptr(value),
            phantom: std::marker::PhantomData,
        }
    }

    /// Turn this `Narc` into the `Send` version `Snarc`.
    pub fn into_send(mut self) -> Snarc<T> {
        let snarc = Snarc {
            ptr: self.ptr,
            phantom: self.phantom,
        };

        self.ptr = ptr::null_mut();

        snarc
    }

    /// Turn this parameterized `Narc` the unparameterized `ErasedNarc`.
    pub fn into_erased(self) -> ErasedNarc
    where
        T: Send + 'static,
    {
        self.into_send().into_erased().into_unsend()
    }

    #[inline(always)]
    fn inner(&self) -> &SnarcBox<T> {
        unsafe { &*self.ptr }
    }

    /// Creates a new non-owning reference to the inner value.
    pub fn new_ref(&self) -> SnarcRef<T> {
        let inner = self.inner();

        inner.count.set(inner.count.get() + 1);

        SnarcRef {
            ptr: self.ptr,
            phantom: Default::default(),
        }
    }
}

impl<T: Send + 'static> From<Narc<T>> for ErasedSnarc {
    fn from(narc: Narc<T>) -> Self {
        narc.into_send().into_erased()
    }
}

impl<T: Send + 'static> From<Narc<T>> for ErasedNarc {
    fn from(narc: Narc<T>) -> Self {
        narc.into_erased()
    }
}

impl<T> Deref for Narc<T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.inner().value
    }
}

impl<T> DerefMut for Narc<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { SnarcBox::into_mut_unchecked(&self.ptr) }
    }
}

impl<T> Drop for Narc<T> {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            {
                self.inner()
                    .thread_local
                    .get_or_default()
                    .set(State::Entered);

                let _guard = scopeguard::guard((), |_| {
                    self.inner()
                        .thread_local
                        .get_or_default()
                        .set(State::Default)
                });

                unsafe {
                    // destroy the contained object
                    ptr::drop_in_place(SnarcBox::into_mut_unchecked(&self.ptr));
                }
            }

            if self.inner().count.get() == 0 {
                unsafe {
                    ptr::addr_of_mut!((*self.ptr).count).drop_in_place();
                    let layout = alloc::Layout::for_value(&*self.ptr);
                    alloc::dealloc(self.ptr.cast(), layout);
                }
            }
        }
    }
}

/// A sendable, non-owning reference-counted pointer to a `T`.
pub struct SnarcRef<T> {
    ptr: *mut SnarcBox<T>,
    phantom: std::marker::PhantomData<SnarcBox<T>>,
}

unsafe impl<T> Send for SnarcRef<T> {}
unsafe impl<T> Sync for SnarcRef<T> {}

impl<T> SnarcRef<T> {
    #[inline(always)]
    fn inner(&self) -> &SnarcBox<T> {
        unsafe { &*self.ptr }
    }

    /// Gets a reference to the inner value.
    ///
    /// Returns `None` if the corresponding owning pointer did not currently
    /// bind the inner value to the current thread.
    pub fn get(&self) -> Option<&T> {
        let inner = self.inner();

        if inner.thread_local.get_or_default().get().is_set() {
            Some(&inner.value)
        } else {
            None
        }
    }
}

impl<T> Clone for SnarcRef<T> {
    fn clone(&self) -> Self {
        let inner = self.inner();

        if inner.thread_local.get_or_default().get().is_set() {
            inner.count.set(inner.count.get() + 1);

            SnarcRef {
                ptr: self.ptr,
                phantom: Default::default(),
            }
        } else {
            panic!("SnarcRef::clone() outside of Snarc::enter(…)")
        }
    }
}

impl<T> Drop for SnarcRef<T> {
    fn drop(&mut self) {
        let inner = self.inner();

        if inner.thread_local.get_or_default().get().is_set() {
            inner.count.set(inner.count.get() - 1);
        } else {
            #[cfg(debug_assertions)]
            panic!("SnarcRef::drop() outside of Snarc::enter(…)")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Narc;
    use super::Snarc;
    use super::SnarcRef;

    crate::tests::tests!(Snarc, Narc, SnarcRef);
}
