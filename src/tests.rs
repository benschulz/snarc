macro_rules! tests {
    ($send:ident, $unsend:ident, $ref:ident) => {
        mod _tests {
            use std::cell::RefCell;
            use std::rc::Rc;
            use std::sync::Arc;
            use std::sync::Mutex;

            use crate::ErasedNarc;
            use crate::ErasedSnarc;

            use super::$send as Snarc;
            use super::$unsend as Narc;
            use super::$ref as SnarcRef;

            #[test]
            fn snarc_is_send_given_send() {
                static_assertions::assert_impl_all!(Snarc<()>: Send);
                static_assertions::assert_not_impl_all!(Snarc<Rc<()>>: Send);
            }

            #[test]
            fn narc_is_not_send() {
                static_assertions::assert_not_impl_all!(Narc<()>: Send);
            }

            #[test]
            fn snarc_ref_is_send() {
                static_assertions::assert_impl_all!(SnarcRef<()>: Send);
                static_assertions::assert_impl_all!(SnarcRef<Rc<()>>: Send);
            }

            #[test]
            fn snarc_is_sync_given_sync() {
                static_assertions::assert_impl_all!(Snarc<()>: Sync);
                static_assertions::assert_not_impl_all!(Snarc<RefCell<()>>: Sync);
            }

            #[test]
            fn narc_is_sync_given_sync() {
                static_assertions::assert_impl_all!(Narc<()>: Sync);
                static_assertions::assert_not_impl_all!(Narc<RefCell<()>>: Sync);
            }

            #[test]
            fn snarc_ref_is_sync() {
                static_assertions::assert_impl_all!(SnarcRef<()>: Sync);
                static_assertions::assert_impl_all!(SnarcRef<RefCell<()>>: Sync);
            }

            #[test]
            fn snarc_owns_its_value() {
                let mut snarc = Snarc::new(5);

                *snarc += 1;

                assert_eq!(*snarc, 6);

                let mut_ref = &mut *snarc;

                *mut_ref += 1;

                assert_eq!(*snarc, 7);
            }

            #[test]
            fn snarc_may_contain_refs() {
                let mut snarc = Snarc::new(SelfReferential(None));

                *snarc = SelfReferential(Some(snarc.new_ref()));

                drop(snarc);
            }

            #[test]
            fn snarc_refs_return_none_after_drop() {
                let snarc = Snarc::new(());

                let r = snarc.new_ref();

                drop(snarc);

                assert_eq!(r.get(), None);

                Box::leak(Box::new(r));
            }

            #[test]
            fn snarc_into_narc() {
                let snarc = Snarc::new(Droppable::new());
                let tester = snarc.tester();

                assert!(!tester.dropped());

                let narc = snarc.into_unsend();

                assert!(!tester.dropped());

                drop(narc);

                assert!(tester.dropped());
            }

            #[test]
            fn snarc_into_erased() {
                let snarc = Snarc::new(Droppable::new());
                let tester = snarc.tester();

                assert!(!tester.dropped());

                let erased = snarc.into_erased();

                assert!(!tester.dropped());

                drop(erased);

                assert!(tester.dropped());
            }

            #[test]
            fn erased_snarc_from_snarc() {
                let snarc = Snarc::new(Droppable::new());
                let tester = snarc.tester();

                assert!(!tester.dropped());

                let erased = ErasedSnarc::from(snarc);

                assert!(!tester.dropped());

                drop(erased);

                assert!(tester.dropped());
            }

            #[test]
            fn erased_narc_from_snarc() {
                let snarc = Snarc::new(Droppable::new());
                let tester = snarc.tester();

                assert!(!tester.dropped());

                let erased = ErasedNarc::from(snarc);

                assert!(!tester.dropped());

                drop(erased);

                assert!(tester.dropped());
            }

            #[test]
            fn narc_owns_its_value() {
                let mut narc = Narc::new(5);

                *narc += 1;

                assert_eq!(*narc, 6);

                let mut_ref = &mut *narc;

                *mut_ref += 1;

                assert_eq!(*narc, 7);
            }

            #[test]
            fn narc_may_contain_refs() {
                let mut narc = Narc::new(SelfReferential(None));

                *narc = SelfReferential(Some(narc.new_ref()));

                drop(narc);
            }

            #[test]
            fn narc_refs_return_none_after_drop() {
                let narc = Narc::new(());

                let r = narc.new_ref();

                drop(narc);

                assert_eq!(r.get(), None);

                Box::leak(Box::new(r));
            }

            #[test]
            fn narc_into_snarc() {
                let narc = Narc::new(Droppable::new());
                let tester = narc.tester();

                assert!(!tester.dropped());

                let snarc = narc.into_send();

                assert!(!tester.dropped());

                drop(snarc);

                assert!(tester.dropped());
            }

            #[test]
            fn narc_into_erased() {
                let narc = Narc::new(Droppable::new());
                let tester = narc.tester();

                assert!(!tester.dropped());

                let erased = narc.into_erased();

                assert!(!tester.dropped());

                drop(erased);

                assert!(tester.dropped());
            }

            #[test]
            fn erased_snarc_from_narc() {
                let narc = Narc::new(Droppable::new());
                let tester = narc.tester();

                assert!(!tester.dropped());

                let erased = ErasedSnarc::from(narc);

                assert!(!tester.dropped());

                drop(erased);

                assert!(tester.dropped());
            }

            #[test]
            fn erased_narc_from_narc() {
                let narc = Narc::new(Droppable::new());
                let tester = narc.tester();

                assert!(!tester.dropped());

                let erased = ErasedNarc::from(narc);

                assert!(!tester.dropped());

                drop(erased);

                assert!(tester.dropped());
            }

            #[test]
            fn snarc_ref() {
                let mut snarc = Snarc::new(5);

                let snarc_ref = snarc.new_ref();

                assert_eq!(snarc_ref.get(), None);

                snarc.enter(|v| {
                    assert_eq!(*v, 5);
                    assert_eq!(snarc_ref.get(), Some(&5));

                    drop(snarc_ref);
                })
            }

            #[test]
            fn snarc_ref_is_clonable() {
                let mut snarc = Snarc::new(5);

                let snarc_ref = snarc.new_ref();

                assert_eq!(snarc_ref.get(), None);

                snarc.enter(|v| {
                    assert_eq!(*v, 5);
                    assert_eq!(snarc_ref.get(), Some(&5));
                    assert_eq!(SnarcRef::clone(&snarc_ref).get(), Some(&5));

                    drop(snarc_ref);
                })
            }

            #[test]
            #[should_panic]
            fn clone_snarc_ref_in_invalid_context_panics() {
                let mut snarc = Snarc::new(SelfReferential(None));
                *snarc = SelfReferential(Some(snarc.new_ref()));

                let _should_panic = SnarcRef::clone(snarc.0.as_ref().unwrap());
            }

            #[test]
            #[should_panic]
            #[cfg(debug_assertions)]
            fn drop_snarc_ref_in_invalid_context_panics() {
                let snarc = Snarc::new(5);

                let snarc_ref = snarc.new_ref();

                drop(snarc_ref);
            }

            struct SelfReferential(Option<SnarcRef<SelfReferential>>);

            struct Droppable(Arc<Mutex<bool>>);

            impl Droppable {
                fn new() -> Self {
                    Self(Default::default())
                }

                fn tester(&self) -> DropTester {
                    DropTester(Arc::clone(&self.0))
                }
            }

            impl Drop for Droppable {
                fn drop(&mut self) {
                    let mut dropped = self.0.lock().unwrap();
                    *dropped = true;
                }
            }

            struct DropTester(Arc<Mutex<bool>>);

            impl DropTester {
                fn dropped(&self) -> bool {
                    let dropped = self.0.lock().unwrap();
                    *dropped
                }
            }
        }
    }
}

pub(crate) use tests;
