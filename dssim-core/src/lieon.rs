//! Shim for single-threaded rayon replacement

// Unlike other code in this project, this file is licensed
// under both CC0 and AGPL-3.0, whichever you prefer.
// <https://creativecommons.org/public-domain/cc0/>

pub mod prelude {
    pub use super::*;
    pub use itertools::Itertools;
}

pub trait ParIterator: Sized {
    fn with_min_len(self, _one: usize) -> Self { self }
    fn with_max_len(self, _one: usize) -> Self { self }
    fn par_bridge(self) -> Self { self }
}

impl<T: Iterator> ParIterator for T {
}

pub trait ParSliceLie<T> {
    fn par_chunks(&self, n: usize) -> std::slice::Chunks<'_, T>;
}

pub trait ParSliceMutLie<T> {
    fn par_chunks_exact_mut(&mut self, n: usize) -> std::slice::ChunksExactMut<'_, T>;
}

pub trait ParIntoIterLie<T> {
    type IntoIter;
    fn into_par_iter(self) -> Self::IntoIter;
}

pub trait ParIterLie<T> {
    type Iter;
    fn par_iter(&self) -> Self::Iter;
}

pub trait ParIterMutLie<'a, T> {
    type Iter;
    fn par_iter_mut(&'a mut self) -> Self::Iter;
}

pub fn join<A, B>(a: impl FnOnce() -> A, b: impl FnOnce() -> B) -> (A, B) {
    let a = a();
    let b = b();
    (a, b)
}

impl<'a, T> ParSliceLie<T> for &'a [T] {
    fn par_chunks(&self, n: usize) -> std::slice::Chunks<'_, T> {
        self.chunks(n)
    }
}

impl<'a, T> ParSliceLie<T> for &'a mut [T] {
    fn par_chunks(&self, n: usize) -> std::slice::Chunks<'_, T> {
        self.chunks(n)
    }
}

impl<'a, T> ParSliceMutLie<T> for &'a mut [T] {
    fn par_chunks_exact_mut(&mut self, n: usize) -> std::slice::ChunksExactMut<'_, T> {
        self.chunks_exact_mut(n)
    }
}

impl<'a, T> ParIterLie<T> for &'a [T] {
    type Iter = std::slice::Iter<'a, T>;

    fn par_iter(&self) -> Self::Iter {
        self.iter()
    }
}

impl<'a, T> ParIterMutLie<'a, T> for &'a mut [T] {
    type Iter = std::slice::IterMut<'a, T>;

    fn par_iter_mut(&'a mut self) -> Self::Iter {
        self.iter_mut()
    }
}

impl<T> ParIntoIterLie<T> for Vec<T> {
    type IntoIter = std::vec::IntoIter<T>;

    fn into_par_iter(self) -> Self::IntoIter {
        self.into_iter()
    }
}
