use std::fmt;
use std::ops::Range;
use once_cell::sync::OnceCell;
use std::cell::Cell;

pub type Place = u16;

#[derive(Copy, Clone, Debug)]
pub struct PlaceGroup {
    size: usize,
}

impl fmt::Display for PlaceGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl PlaceGroup {
    pub fn new(size: usize) -> Self {
        PlaceGroup { size }
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn iter(&self) -> Range<Place> {
        0..self.size as Place
    }

    pub fn is_empty(&self) -> bool {
        // otherwise clippy complains
        self.size == 0
    }
}

impl IntoIterator for PlaceGroup {
    type Item = Place;
    type IntoIter = Range<Place>;
    fn into_iter(self) -> Self::IntoIter {
        0..self.size as Place
    }
}

// TODO: remote pub. That reqires refactoring for test cases
pub(crate) static HERE_STATIC: OnceCell<Place> = OnceCell::new();
pub(crate) static WORLD_SIZE_STATIC: OnceCell<usize> = OnceCell::new();

thread_local! {
    pub(crate) static HERE_LOCAL: Cell<Option<Place>> = Cell::new(None);
    static WORLD_SIZE: Cell<Option<usize>> = Cell::new(None);
}

pub(crate) fn init_here(here: Place) {
    HERE_STATIC.set(here).unwrap();
}

pub(crate) fn init_world_size(size: usize){
    WORLD_SIZE_STATIC.set(size).unwrap();
}

pub fn here() -> Place {
    HERE_LOCAL.with(|h| match h.get() {
        Some(p) => p,
        None => {
            let p = HERE_STATIC.get().expect("here place id is not initialized");
            h.set(Some(*p));
            here()
        }
    })
}

pub fn world_size() -> usize{
    WORLD_SIZE.with(|h| match h.get() {
        Some(p) => p,
        None => {
            let p = WORLD_SIZE_STATIC.get().expect("world size is not initialized");
            h.set(Some(*p));
            world_size()
        }
    })
}


#[cfg(test)]
mod test {

    use super::*;
    use std::thread;

    #[test]
    pub fn test_place_group() {
        let pg = PlaceGroup::new(5);
        assert_eq!(pg.len(), 5);
        assert_eq!(
            pg.iter().map(|a| a).collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4]
        );
        assert_eq!(pg.into_iter().collect::<Vec<_>>(), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn test_here() {
        use crate::global_id::test::TEST_HERE;
        use crate::global_id::test::TestGuardForStatic;
        let _a = TestGuardForStatic::new();
        let mut threads = vec![];
        for _ in 0..8 {
            threads.push(thread::spawn(|| {
                assert!(HERE_LOCAL.with(|h| h.get().is_none()));
                assert_eq!(here(), TEST_HERE);
                assert!(HERE_LOCAL.with(|h| h.get().is_some()));
                assert_eq!(here(), TEST_HERE);
            }));
        }
        for t in threads {
            t.join().unwrap();
        }
    }
}
