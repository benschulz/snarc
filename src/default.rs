#[doc(hidden)]
pub use scopeguard;

/// Defines three structs, each named by one parameter.
///
///  - `$send`, a sendable owning/strong reference,
///  - `$unsend`, an unsendable owning/strong reference, and
///  - `$ref`, a sendable weak reference.
///
/// Note that this macro declares a `thread_local!` that is is shared by all
/// instances. That means that instances cannot bind their value to the current
/// thread at the same time. If this is too restrictive, enable the
/// `thread-local` feature and use the structs defined in the `thread_local`
/// module.
#[macro_export]
macro_rules! snarc {
    ($send:ident, $unsend:ident, $ref:ident $(, $expect:literal)?) => {
        pub use _snarc_impl::$send;
        pub use _snarc_impl::$unsend;
        pub use _snarc_impl::$ref;

        mod _snarc_impl {
            use std::alloc;
            use std::ops::Deref;
            use std::ops::DerefMut;
            use std::ptr;

            use $crate::Context;
            use $crate::ErasedNarc;
            use $crate::ErasedSnarc;
            use $crate::State;

            thread_local!(static THREAD_LOCAL: std::cell::Cell<State> = Default::default());

            struct SnarcBox<T> {
                count: std::cell::Cell<usize>,
                value: T,
            }

            impl<T> SnarcBox<T> {
                fn new_ptr(value: T) -> *mut Self {
                    Box::leak(Box::new(Self {
                        count: std::cell::Cell::new(0),
                        value,
                    }))
                }
            }

            pub struct $send<T> {
                ptr: *mut SnarcBox<T>,
                phantom: std::marker::PhantomData<SnarcBox<T>>,
            }

            unsafe impl<T: Send> Send for $send<T> {}
            unsafe impl<T: Sync> Sync for $send<T> {}

            impl<T> $send<T> {
                /// Creates a new `
                #[doc = stringify!($send)]
                /// ` with the given inner `value`.
                pub fn new(value: T) -> Self {
                    Self {
                        ptr: SnarcBox::new_ptr(value),
                        phantom: std::marker::PhantomData,
                    }
                }

                /// Turn this `
                #[doc = stringify!($send)]
                /// ` into the `!Send` version `
                #[doc = stringify!($unsend)]
                /// `.
                pub fn into_unsend(mut self) -> $unsend<T> {
                    let narc = $unsend {
                        ptr: self.ptr,
                        phantom: self.phantom,
                    };

                    self.ptr = ptr::null_mut();

                    narc
                }

                /// Turn this parameterized `
                #[doc = stringify!($send)]
                /// ` the unparameterized `ErasedSnarc`.
                pub fn into_erased(self) -> ErasedSnarc
                where
                    T: Send + 'static,
                {
                    let snarc: Box<dyn Context + Send + 'static> = Box::new(self);
                    ErasedSnarc::from(snarc)
                }

                #[inline(always)]
                fn inner(&self) -> &SnarcBox<T> {
                    unsafe { &*self.ptr }
                }

                #[inline]
                unsafe fn get_mut_unchecked(this: &mut Self) -> &mut T {
                    &mut (*this.ptr).value
                }

                /// Creates a new non-owning reference to the inner value.
                pub fn new_ref(&self) -> $ref<T> {
                    let inner = self.inner();

                    inner.count.set(inner.count.get() + 1);

                    $ref {
                        ptr: self.ptr,
                        phantom: Default::default(),
                    }
                }

                /// Temporarily bind the inner value to this thread and evaluate `f`
                /// within that context.
                pub fn enter<F, R>(&mut self, f: F) -> R
                where
                    F: FnOnce(&T) -> R,
                {
                    THREAD_LOCAL.with(|c| {
                        if c.get() == State::Entered {
                            panic!(concat!(
                                "Another ",
                                stringify!($send),
                                " is already entered."
                            ))
                        }

                        c.set(State::Entered);
                    });

                    let _guard = $crate::scopeguard::guard((), |_| {
                        THREAD_LOCAL.with(|c| c.set(State::Default));
                    });

                    f(&self.inner().value)
                }
            }

            impl<T: Send + 'static> From<$send<T>> for ErasedSnarc {
                fn from(snarc: $send<T>) -> Self {
                    snarc.into_erased()
                }
            }

            impl<T: Send + 'static> From<$send<T>> for ErasedNarc {
                fn from(snarc: $send<T>) -> Self {
                    snarc.into_unsend().into_erased()
                }
            }

            impl<T> Context for $send<T> {
                fn set(&mut self, v: State) {
                    THREAD_LOCAL.with(|c| {
                        if v == State::Entered && c.get() == State::Entered {
                            panic!(concat!(
                                "Another ",
                                stringify!($send),
                                " is already entered."
                            ))
                        }

                        c.set(v);
                    });
                }
            }

            impl<T> Deref for $send<T> {
                type Target = T;

                #[inline(always)]
                fn deref(&self) -> &Self::Target {
                    &self.inner().value
                }
            }

            impl<T> DerefMut for $send<T> {
                #[inline(always)]
                fn deref_mut(&mut self) -> &mut Self::Target {
                    unsafe { Self::get_mut_unchecked(self) }
                }
            }

            impl<T> Drop for $send<T> {
                fn drop(&mut self) {
                    if !self.ptr.is_null() {
                        THREAD_LOCAL.with(|c| {
                            if c.get() == State::Entered {
                                panic!(concat!(
                                    "Another ",
                                    stringify!($send),
                                    " is already entered."
                                ))
                            }

                            c.set(State::Entered)
                        });

                        let _guard = $crate::scopeguard::guard((), |_| {
                            THREAD_LOCAL.with(|c| c.set(State::Default));
                        });

                        unsafe {
                            // destroy the contained object
                            ptr::drop_in_place(Self::get_mut_unchecked(self));
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
            /// When `
            #[doc = stringify!($unsend)]
            /// ` is used exclusively, i.e., never `
            #[doc = stringify!($send)]
            /// `, then `Rc` should
            /// likely be used instead.
            pub struct $unsend<T> {
                ptr: *mut SnarcBox<T>,
                phantom: std::marker::PhantomData<SnarcBox<T>>,
            }

            unsafe impl<T: Sync> Sync for $unsend<T> {}

            impl<T> $unsend<T> {
                /// Creates a new `
                #[doc = stringify!($unsend)]
                /// ` with the given inner `value`.
                pub fn new(value: T) -> Self {
                    Self {
                        ptr: SnarcBox::new_ptr(value),
                        phantom: std::marker::PhantomData,
                    }
                }

                /// Turn this `
                #[doc = stringify!($unsend)]
                /// ` into the `Send` version `
                #[doc = stringify!($send)]
                /// `.
                pub fn into_send(mut self) -> $send<T> {
                    let snarc = $send {
                        ptr: self.ptr,
                        phantom: self.phantom,
                    };

                    self.ptr = ptr::null_mut();

                    snarc
                }

                /// Turn this parameterized `
                #[doc = stringify!($unsend)]
                /// ` the unparameterized `ErasedNarc`.
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

                #[inline]
                unsafe fn get_mut_unchecked(this: &mut Self) -> &mut T {
                    &mut (*this.ptr).value
                }

                /// Creates a new non-owning reference to the inner value.
                pub fn new_ref(&self) -> $ref<T> {
                    let inner = self.inner();

                    inner.count.set(inner.count.get() + 1);

                    $ref {
                        ptr: self.ptr,
                        phantom: Default::default(),
                    }
                }
            }

            impl<T: Send + 'static> From<$unsend<T>> for ErasedSnarc {
                fn from(narc: $unsend<T>) -> Self {
                    narc.into_send().into_erased()
                }
            }

            impl<T: Send + 'static> From<$unsend<T>> for ErasedNarc {
                fn from(narc: $unsend<T>) -> Self {
                    narc.into_erased()
                }
            }

            impl<T> Deref for $unsend<T> {
                type Target = T;

                #[inline(always)]
                fn deref(&self) -> &Self::Target {
                    &self.inner().value
                }
            }

            impl<T> DerefMut for $unsend<T> {
                #[inline(always)]
                fn deref_mut(&mut self) -> &mut Self::Target {
                    unsafe { Self::get_mut_unchecked(self) }
                }
            }

            impl<T> Drop for $unsend<T> {
                fn drop(&mut self) {
                    if !self.ptr.is_null() {
                        THREAD_LOCAL.with(|c| {
                            if c.get() == State::Entered {
                                panic!(concat!(
                                    "Another ",
                                    stringify!($send),
                                    " is already entered."
                                ))
                            }

                            c.set(State::Entered)
                        });

                        let _guard = $crate::scopeguard::guard((), |_| {
                            THREAD_LOCAL.with(|c| c.set(State::Default));
                        });

                        unsafe {
                            // destroy the contained object
                            ptr::drop_in_place(Self::get_mut_unchecked(self));
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

            pub struct $ref<T> {
                ptr: *mut SnarcBox<T>,
                phantom: std::marker::PhantomData<SnarcBox<T>>,
            }

            unsafe impl<T> Send for $ref<T> {}
            unsafe impl<T> Sync for $ref<T> {}

            impl<T> $ref<T> {
                #[inline(always)]
                fn inner(&self) -> &SnarcBox<T> {
                    unsafe { &*self.ptr }
                }

                pub fn get(&self) -> Option<&T> {
                    let inner = self.inner();

                    if THREAD_LOCAL.with(|c| c.get().is_set()) {
                        Some(&inner.value)
                    } else {
                        None
                    }
                }

                $(
                    pub fn expect(&self) -> &T {
                        self.get().expect($expect)
                    }
                )?
            }

            impl<T> Clone for $ref<T> {
                fn clone(&self) -> Self {
                    if THREAD_LOCAL.with(|c| c.get().is_set()) {
                        let inner = self.inner();

                        inner.count.set(inner.count.get() + 1);

                        Self {
                            ptr: self.ptr,
                            phantom: Default::default(),
                        }
                    } else {
                        panic!(concat!(
                            stringify!($ref),
                            "::clone() outside of ",
                            stringify!($send),
                            "::enter(…)"
                        ))
                    }
                }
            }

            impl<T> Drop for $ref<T> {
                fn drop(&mut self) {
                    if THREAD_LOCAL.with(|c| c.get().is_set()) {
                        let inner = self.inner();

                        inner.count.set(inner.count.get() - 1);
                    } else {
                        #[cfg(debug_assertions)]
                        panic!(concat!(
                            stringify!($ref),
                            "::drop() outside of ",
                            stringify!($send),
                            "::enter(…)"
                        ))
                    }
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {
    crate::snarc!(Snarc, Narc, SnarcRef, "expectation");

    crate::tests::tests!(Snarc, Narc, SnarcRef);
}
