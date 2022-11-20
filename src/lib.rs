//! Snarc provides a Sendable Non-Atomically Reference-Counted smart-pointer.
//!
//! [![crates.io][crates.io-badge]][crates.io-url]
//! [![docs.rs][docs.rs-badge]][docs.rs-url]
//! [![MIT licensed][mit-badge]][mit-url]
//!
//! [crates.io-badge]: https://img.shields.io/crates/v/snarc.svg
//! [crates.io-url]: https://crates.io/crates/snarc
//! [docs.rs-badge]: https://docs.rs/snarc/badge.svg
//! [docs.rs-url]: https://docs.rs/snarc
//! [mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
//! [mit-url]: https://github.com/benschulz/snarc/blob/master/LICENSE
//!
//! # How does it work
//!
//! In order to be both sendable _and_ non-atomically reference counted,
//! trade-offs must be made. Those trade-offs are as follows.
//!
//! - There is only one strong/owning reference and arbitrarily many weak
//!   references. By invoking the `enter` method of the strong/owning reference
//!   its value may be temporarily bound to the current thread.
//!
//! - Weak references may only be created and dropped within the `enter` context
//!   of a strong/owning reference. This ensures that the required counter
//!   increments and decrements are race-free.
//!
//! - Calling the `get` method on a weak reference returns an `Option<&T>`, that
//!   is `Some(&t)` iff called from within the `enter` context of a strong
//!   reference.
//!
//! # What is it good for?
//!
//! The use case that motivated the implementation of snarc is quite niche. It
//! looks something like the following.
//!
//! ```
//! # use std::future::Future;
//! # use futures::stream::{FuturesUnordered, StreamExt};
//! # struct X {}
//! # impl X { fn method(&mut self) -> impl Future<Output = ()> { async {} } }
//! #
//! # let mut x = X {};
//! #
//! // We have an async task.
//! let task = async {
//!     // This task is creating and executing subtasks.
//!     let subtasks = FuturesUnordered::new();
//!
//!     // `x.method()` is returning 'static Futures that share mutable state
//!     subtasks.push(x.method());
//!     subtasks.push(x.method());
//!
//!     // Somewhere within the same task, the subtasks are executed.
//!     subtasks.for_each(|x| async { /* ... */ });
//! };
//! ```
//!
//! Because the futures returned by `x.method()` share mutable state, that state
//! must be wrapped in a `RefCell`. And because the futures also have a
//! `'static` lifetime, that `RefCell` must be wrapped by a reference counted
//! smart pointer.
//!
//! ## Alternatives
//!
//! Given the problem statement above, here are the alternative solutions.
//!
//! ### Use `&RefCell<T>` after all
//!
//! This isn't really a solution to the problem statement, but maybe you can
//! relax your requirements? Maybe you don't need the returned futures to have a
//! `'static` lifetime?
//!
//! **Advantages**
//!  - no overhead/maximally efficient
//!
//! **Drawbacks**
//!  - `task` will be `!Send`
//!  - addresses a different problem
//!
//! ### Use `Rc<RefCell<T>>`
//!
//! **Advantages**
//!  - highly efficient, minor overhead of reference counting
//!
//! **Drawbacks**
//!  - `task` will be `!Send`
//!
//! ### Use `Arc<Mutex<T>>`
//!
//! **Advantages**
//!  - `task` will be `Send`
//!  - highly ergonomical
//!  - subtasks can even be turned into tasks of their own and executed on a
//!    different thread
//!
//! **Drawbacks**
//!  - inefficient, due to locking overhead
//!
//! ### Use `Snarc<RefCell<T>` and `SnarcRef<RefCell<T>>`
//!
//! **Advantages**
//!  - highly efficient, minor overhead of reference counting
//!  - `task` can be `Send`
//!
//! **Drawbacks**
//!  - the ergonomics are iffy

//
// Lint configuration
#![warn(rust_2018_idioms)]
#![warn(clippy::wildcard_imports)]
#![warn(missing_docs)]

#[doc(hidden)]
#[cfg(feature = "default")]
pub mod default;

#[cfg(feature = "thread_local")]
pub mod thread_local;

#[doc(hidden)]
#[cfg(feature = "default")]
pub use default::*;

#[cfg(test)]
mod tests;

#[doc(hidden)]
pub trait Context {
    fn set(&mut self, v: State);
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Default,
    Unsend,
    Entered,
}

impl State {
    pub fn is_set(&self) -> bool {
        match self {
            State::Default => false,
            State::Unsend => true,
            State::Entered => true,
        }
    }
}

impl Default for State {
    fn default() -> Self {
        State::Default
    }
}

/// A `Snarc<T>` whose type argument was erased.
///
/// `ErasedSnarc`s cannot be used to access the (type-erased) inner value. They
/// can only be used to temporarily bind the value to the current thread using
/// the [`enter`][ErasedSnarc::enter()] method.
pub struct ErasedSnarc {
    inner: Box<dyn Context + Send + 'static>,
}

unsafe impl Send for ErasedSnarc {}
unsafe impl Sync for ErasedSnarc {}

impl ErasedSnarc {
    /// Turn this into an unsendable `ErasedNarc` bound to the current thread.
    pub fn into_unsend(self) -> ErasedNarc {
        self.inner.into()
    }

    /// Temporarily bind the inner value to this thread and evaluate `f` within
    /// that context.
    pub fn enter<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        self.inner.set(State::Entered);

        let _guard = scopeguard::guard((), |_| {
            self.inner.set(State::Default);
        });

        f()
    }
}

impl From<Box<dyn Context + Send + 'static>> for ErasedSnarc {
    fn from(inner: Box<dyn Context + Send + 'static>) -> Self {
        Self { inner }
    }
}

impl From<ErasedNarc> for ErasedSnarc {
    fn from(narc: ErasedNarc) -> Self {
        narc.into_send()
    }
}

/// A unsendable `Snarc<T>` whose type argument was erased.
///
/// `ErasedNarc`s cannot be used to access the (type-erased) inner value.
pub struct ErasedNarc {
    inner: Box<dyn Context + Send + 'static>,
    _unsend: std::marker::PhantomData<std::sync::MutexGuard<'static, ()>>,
}

impl ErasedNarc {
    /// Turn this into a sendable `ErasedSnarc`.
    pub fn into_send(mut self) -> ErasedSnarc {
        self.inner.set(State::Default);
        self.inner.into()
    }
}

impl From<Box<dyn Context + Send + 'static>> for ErasedNarc {
    fn from(mut inner: Box<dyn Context + Send + 'static>) -> Self {
        inner.set(State::Unsend);
        Self {
            inner,
            _unsend: Default::default(),
        }
    }
}

impl From<ErasedSnarc> for ErasedNarc {
    fn from(snarc: ErasedSnarc) -> Self {
        snarc.into_unsend()
    }
}
