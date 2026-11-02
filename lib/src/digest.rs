use c2pa::SigningAlg;
use sha2::{Digest, Sha256, Sha384, Sha512};

/// Hashes `data` with the digest that matches the signature algorithm.
pub(crate) fn digest(algorithm: SigningAlg, data: &[u8]) -> c2pa::Result<Vec<u8>> {
    match algorithm {
        SigningAlg::Ps256 | SigningAlg::Es256 => Ok(Sha256::digest(data).to_vec()),
        SigningAlg::Ps384 | SigningAlg::Es384 => Ok(Sha384::digest(data).to_vec()),
        SigningAlg::Ps512 | SigningAlg::Es512 => Ok(Sha512::digest(data).to_vec()),
        _ => Err(c2pa::Error::CoseSignatureAlgorithmNotSupported),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_length_matches_algorithm() {
        assert_eq!(digest(SigningAlg::Ps256, b"abc").unwrap().len(), 32);
        assert_eq!(digest(SigningAlg::Es384, b"abc").unwrap().len(), 48);
        assert_eq!(digest(SigningAlg::Ps512, b"abc").unwrap().len(), 64);
        assert!(digest(SigningAlg::Ed25519, b"abc").is_err());
    }
}
