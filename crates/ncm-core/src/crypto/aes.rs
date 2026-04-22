use aes::Aes128;
use cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyInit};
use ecb::Decryptor;

use crate::error::{NcmError, Result};

type Aes128EcbDec = Decryptor<Aes128>;

pub fn aes128_ecb_decrypt(key: &[u8; 16], ciphertext: &[u8]) -> Result<Vec<u8>> {
    Aes128EcbDec::new(key.into())
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|e| NcmError::Aes(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::Aes128;
    use cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyInit};
    use ecb::Encryptor;

    type Aes128EcbEnc = Encryptor<Aes128>;

    #[test]
    fn round_trip() {
        let key = *b"YELLOW SUBMARINE";
        let plaintext = b"Hello NCM world, this spans multiple AES blocks.";

        let ciphertext = Aes128EcbEnc::new(&key.into())
            .encrypt_padded_vec_mut::<Pkcs7>(plaintext);
        let decrypted = aes128_ecb_decrypt(&key, &ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn invalid_ciphertext_errors() {
        let key = [0u8; 16];
        // Length not a multiple of 16 must fail.
        let result = aes128_ecb_decrypt(&key, &[0u8; 7]);
        assert!(result.is_err());
    }
}
