use std::sync::atomic::{AtomicU64, Ordering};

static GLOBAL_SEED_COUNTER: AtomicU64 = AtomicU64::new(0xDEAD_BEEF_CAFE_BABE);

#[derive(Debug, Clone)]
pub struct FastRng {
    state: u64,
}

impl Default for FastRng {
    fn default() -> Self {
        let seed = GLOBAL_SEED_COUNTER.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
        Self::new(seed)
    }
}

impl FastRng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x853C_49E6_748F_EA9B } else { seed },
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 12;
        x ^= x >> 25;
        x ^= x << 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn gen_range_f64(&mut self, min: f64, max: f64) -> f64 {
        let frac = (self.next_u64() as f64) / (u64::MAX as f64);
        min + frac * (max - min)
    }

    pub fn gen_range_i8(&mut self, min: i8, max: i8) -> i8 {
        let span = (max as i16 - min as i16 + 1).max(1) as u64;
        let v = (self.next_u64() % span) as i16;
        (min as i16 + v) as i8
    }

    pub fn gen_bool(&mut self, prob: f64) -> bool {
        self.gen_range_f64(0.0, 1.0) < prob
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determinism() {
        let mut rng1 = FastRng::new(42);
        let mut rng2 = FastRng::new(42);
        assert_eq!(rng1.next_u64(), rng2.next_u64());
        assert_eq!(rng1.gen_range_f64(10.0, 20.0), rng2.gen_range_f64(10.0, 20.0));
    }

    #[test]
    fn test_range_bounds() {
        let mut rng = FastRng::new(12345);
        for _ in 0..1000 {
            let f = rng.gen_range_f64(-5.0, 5.0);
            assert!(f >= -5.0 && f < 5.0);

            let i = rng.gen_range_i8(-10, 10);
            assert!(i >= -10 && i <= 10);
        }
    }
}
