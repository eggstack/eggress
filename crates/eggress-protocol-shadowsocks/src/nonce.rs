use crate::ShadowsocksError;

#[derive(Debug, Clone)]
pub struct NonceCounter {
    nonce_size: usize,
    counter: u64,
}

impl NonceCounter {
    pub fn new(nonce_size: usize) -> Self {
        Self {
            nonce_size,
            counter: 0,
        }
    }

    pub fn starting_at(nonce_size: usize, counter: u64) -> Self {
        Self {
            nonce_size,
            counter,
        }
    }

    pub fn current(&self) -> Vec<u8> {
        let mut buf = vec![0u8; self.nonce_size];
        let start = self.nonce_size.saturating_sub(8);
        buf[start..].copy_from_slice(&self.counter.to_be_bytes());
        buf
    }

    pub fn advance(&mut self) -> Result<(), ShadowsocksError> {
        self.counter = self
            .counter
            .checked_add(1)
            .ok_or_else(|| ShadowsocksError::Other("nonce counter overflow".into()))?;
        Ok(())
    }

    pub fn nonce_size(&self) -> usize {
        self.nonce_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nonce_starts_at_zero() {
        let nonce = NonceCounter::new(12);
        assert_eq!(nonce.current(), vec![0u8; 12]);
    }

    #[test]
    fn test_nonce_increments() {
        let mut nonce = NonceCounter::new(12);
        nonce.advance().unwrap();
        let bytes = nonce.current();
        assert_eq!(bytes.len(), 12);
        assert_eq!(&bytes[..4], &[0, 0, 0, 0]);
        assert_eq!(&bytes[4..], &1u64.to_be_bytes());
    }

    #[test]
    fn test_nonce_advance_multiple() {
        let mut nonce = NonceCounter::new(12);
        for i in 1u64..=10 {
            nonce.advance().unwrap();
            let bytes = nonce.current();
            assert_eq!(&bytes[4..], &i.to_be_bytes());
        }
    }

    #[test]
    fn test_nonce_overflow_returns_error() {
        let mut nonce = NonceCounter::new(12);
        nonce.counter = u64::MAX;
        assert!(nonce.advance().is_err());
    }
}
