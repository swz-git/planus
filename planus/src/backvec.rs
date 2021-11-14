use std::{alloc::Layout, mem::MaybeUninit, ptr::NonNull};

pub struct BackVec {
    // This is a `Vec<u8>`, that is written from the back instead of the fron.
    ptr: NonNull<u8>,
    // Offset of the last written byte
    offset: usize,
    capacity: usize,
}

impl std::fmt::Debug for BackVec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl BackVec {
    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(16);
        Self {
            ptr: unsafe {
                NonNull::new(std::alloc::alloc(
                    Layout::from_size_align(capacity, 1).unwrap(),
                ))
                .unwrap()
            },
            offset: capacity,
            capacity,
        }
    }

    pub fn clear(&mut self) {
        self.offset = self.capacity;
    }

    pub fn len(&self) -> usize {
        debug_assert!(self.capacity >= self.offset);
        self.capacity.wrapping_sub(self.offset)
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr().add(self.offset), self.len()) }
    }

    #[inline]
    pub fn reserve(&mut self, capacity: usize) {
        if capacity > self.offset {
            self.grow(capacity);
            assert!(capacity <= self.offset);
        }
    }

    fn grow(&mut self, capacity: usize) {
        let len = self.len();
        let needed = len.checked_add(capacity).unwrap();
        let new_capacity = needed.max(self.capacity.saturating_mul(2));
        let new_offset = new_capacity.checked_sub(len).unwrap();

        unsafe {
            let new_ptr = NonNull::new(std::alloc::alloc(
                Layout::from_size_align(new_capacity, 1).unwrap(),
            ))
            .unwrap();

            std::ptr::copy_nonoverlapping(
                self.ptr.as_ptr().add(self.offset),
                new_ptr.as_ptr().add(new_offset),
                len,
            );
            let old_ptr = std::mem::replace(&mut self.ptr, new_ptr);
            std::alloc::dealloc(
                old_ptr.as_ptr(),
                Layout::from_size_align_unchecked(self.capacity, 1),
            );
            self.capacity = new_capacity;
            self.offset = new_offset;
        }
        assert!(capacity <= self.offset);
    }

    #[inline]
    pub fn extend_from_slice(&mut self, buffer: &[u8]) {
        self.reserve(buffer.len());
        let new_offset = self.offset.wrapping_sub(buffer.len());
        unsafe {
            std::ptr::copy_nonoverlapping(
                buffer.as_ptr(),
                self.ptr.as_ptr().add(new_offset),
                buffer.len(),
            )
        }
        self.offset = new_offset;
    }

    #[inline]
    pub fn extend_with_zeros(&mut self, count: usize) {
        self.reserve(count);
        let new_offset = self.offset.wrapping_sub(count);
        unsafe { std::ptr::write_bytes(self.ptr.as_ptr().add(new_offset), 0, count) }
        self.offset = new_offset;
    }

    pub unsafe fn extend_write(&mut self, count: usize, f: impl FnOnce(&mut [MaybeUninit<u8>])) {
        self.reserve(count);
        let new_offset = self.offset.wrapping_sub(count);
        let ptr = self.ptr.as_ptr().add(new_offset) as *mut MaybeUninit<u8>;
        let slice = std::slice::from_raw_parts_mut(ptr, count);
        f(slice);
        self.offset = new_offset;
    }
}

impl Drop for BackVec {
    fn drop(&mut self) {
        unsafe {
            std::alloc::dealloc(
                self.ptr.as_ptr(),
                Layout::from_size_align_unchecked(self.capacity, 1),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{thread_rng, Rng};

    #[test]
    fn test_backvec() {
        let mut rng = thread_rng();
        let mut vec = BackVec::with_capacity(rng.gen::<usize>() % 64);
        let mut slice = [0; 50];
        let mut saved = Vec::new();
        for _ in 0..1_000_000 {
            assert!(vec.len() <= vec.capacity);
            assert_eq!(vec.as_slice().len(), vec.len());

            match rng.gen::<u32>() % 20 {
                0 | 1 => {
                    let old_capacity = vec.capacity;
                    vec.clear();
                    assert_eq!(vec.capacity, old_capacity);
                    assert_eq!(vec.len(), 0);
                }
                2 => {
                    let capacity = rng.gen::<usize>() % 64;
                    vec = BackVec::with_capacity(capacity);
                    assert_eq!(vec.len(), 0);
                    assert_eq!(vec.capacity, capacity.max(16));
                }
                _ => {
                    saved.clear();
                    saved.extend_from_slice(vec.as_slice());

                    let count = rng.gen::<usize>() % slice.len();
                    let new_len = vec.len() + count;
                    let old_capacity = vec.capacity;

                    if rng.gen() {
                        for p in &mut slice[..count] {
                            *p = rng.gen();
                        }
                        vec.extend_from_slice(&slice[..count]);
                        assert_eq!(&vec.as_slice()[..count], &slice[..count]);
                    } else {
                        vec.extend_with_zeros(count);
                        assert!(vec.as_slice()[..count].iter().all(|&b| b == 0));
                    }
                    assert_eq!(vec.len(), new_len);
                    if new_len <= old_capacity {
                        assert_eq!(vec.capacity, old_capacity);
                    } else {
                        assert_eq!(vec.capacity, new_len.max(old_capacity * 2));
                    }
                    assert_eq!(&vec.as_slice()[count..], saved);
                }
            }
        }
    }
}
