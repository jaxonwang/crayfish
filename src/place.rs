use std::fmt;
use std::ops::Range;

pub type Place = u32;

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

#[cfg(test)]
mod test {

    use super::*;
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
}
