mod acs;
mod auth;
mod p7b;
mod sign;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

pub use sign::{SigningOptions, TrustedSigner};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
