use sha2::{Digest, Sha256};

pub fn random_code(digits: u32) -> String {
    let modulus = 10u64.pow(digits);
    let zone = u64::MAX - u64::MAX % modulus;
    loop {
        let value = u64::from_le_bytes(random_bytes());
        if value < zone {
            return format!("{:0width$}", value % modulus, width = digits as usize);
        }
    }
}

pub fn hash_token(token: &str) -> String {
    to_hex(&Sha256::digest(token.as_bytes()))
}

pub fn random_token() -> String {
    to_hex(&random_bytes::<32>())
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    getrandom::getrandom(&mut bytes).expect("system random source");
    bytes
}

pub(crate) fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_have_fixed_width() {
        for _ in 0..100 {
            let code = random_code(6);
            assert_eq!(code.len(), 6);
            assert!(code.bytes().all(|b| b.is_ascii_digit()));
        }
    }

    #[test]
    fn tokens_are_unique_hex() {
        let (a, b) = (random_token(), random_token());
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
