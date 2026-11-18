//! First-fit range allocator over a large GPU buffer, in units of quads.

use std::collections::BTreeMap;

pub struct RangeAlloc {
    free: BTreeMap<u32, u32>,
    pub used: u64,
    pub capacity: u32,
}

impl RangeAlloc {
    pub fn new(capacity: u32) -> Self {
        let mut free = BTreeMap::new();
        free.insert(0, capacity);
        Self { free, used: 0, capacity }
    }

    pub fn alloc(&mut self, n: u32) -> Option<u32> {
        if n == 0 {
            return Some(0);
        }
        let (&start, &len) = self.free.iter().find(|(_, &len)| len >= n)?;
        self.free.remove(&start);
        if len > n {
            self.free.insert(start + n, len - n);
        }
        self.used += n as u64;
        Some(start)
    }

    pub fn free(&mut self, start: u32, n: u32) {
        if n == 0 {
            return;
        }
        self.used -= n as u64;
        let mut s = start;
        let mut len = n;
        if let Some((&ps, &pl)) = self.free.range(..start).next_back() {
            if ps + pl == start {
                self.free.remove(&ps);
                s = ps;
                len += pl;
            }
        }
        if let Some(&nl) = self.free.get(&(start + n)) {
            self.free.remove(&(start + n));
            len += nl;
        }
        self.free.insert(s, len);
    }

    pub fn fragments(&self) -> usize {
        self.free.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_free_coalesces() {
        let mut a = RangeAlloc::new(100);
        let x = a.alloc(10).unwrap();
        let y = a.alloc(20).unwrap();
        let z = a.alloc(30).unwrap();
        assert_eq!((x, y, z), (0, 10, 30));
        a.free(y, 20);
        a.free(x, 10);
        assert_eq!(a.alloc(30), Some(0));
        a.free(0, 30);
        a.free(z, 30);
        assert_eq!(a.fragments(), 1);
        assert_eq!(a.used, 0);
    }
}
