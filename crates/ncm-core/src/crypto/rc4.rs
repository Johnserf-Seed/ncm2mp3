//! NCM's custom stream cipher.
//!
//! KSA is standard RC4-style. PRGA is NOT standard RC4 — each byte is derived
//! from a fixed index `j = (i + 1) & 0xff` rather than from an evolving state,
//! which lets us address arbitrary byte offsets cheaply.

const S_BOX_SIZE: usize = 256;

/// NCM-specific stream cipher. Standard RC4 key scheduling (KSA) followed
/// by a custom pseudo-random generation (PRGA) that indexes `S` by byte
/// offset rather than maintaining evolving state — which is what lets
/// [`apply`](Self::apply) decrypt arbitrary chunks at arbitrary offsets.
pub struct NcmStreamCipher {
    s_box: [u8; S_BOX_SIZE],
}

impl NcmStreamCipher {
    /// Initialize from raw key bytes via the standard RC4 KSA.
    ///
    /// # Panics
    ///
    /// Panics if `key` is empty. Callers that parsed the key out of an NCM
    /// file should already have non-empty key material; empty is a
    /// programmer error, not a data error.
    pub fn new(key: &[u8]) -> Self {
        assert!(!key.is_empty(), "NCM stream key must be non-empty");

        let mut s_box = [0u8; S_BOX_SIZE];
        for (i, slot) in s_box.iter_mut().enumerate() {
            *slot = i as u8;
        }

        let mut j: u8 = 0;
        for i in 0..S_BOX_SIZE {
            j = j.wrapping_add(s_box[i]).wrapping_add(key[i % key.len()]);
            s_box.swap(i, j as usize);
        }

        Self { s_box }
    }

    /// XOR a buffer in place using the NCM stream, where `offset` is the
    /// absolute byte position within the encrypted audio stream.
    pub fn apply(&self, buf: &mut [u8], offset: usize) {
        let s = &self.s_box;
        for (idx, byte) in buf.iter_mut().enumerate() {
            let j = ((offset + idx + 1) & 0xff) as u8;
            let a = s[j as usize];
            let b = s[a.wrapping_add(j) as usize];
            *byte ^= s[a.wrapping_add(b) as usize];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_single_chunk() {
        let key = b"hello-ncm-key";
        let plaintext = b"the quick brown fox jumps over the lazy dog";

        let cipher = NcmStreamCipher::new(key);
        let mut data = plaintext.to_vec();
        cipher.apply(&mut data, 0);
        assert_ne!(&data[..], &plaintext[..], "should encrypt");
        cipher.apply(&mut data, 0);
        assert_eq!(&data[..], &plaintext[..], "should round-trip");
    }

    #[test]
    fn streaming_matches_single_pass() {
        let key = b"streaming-key";
        let data: Vec<u8> = (0..1024).map(|i| (i * 7) as u8).collect();

        let cipher = NcmStreamCipher::new(key);
        let mut single = data.clone();
        cipher.apply(&mut single, 0);

        let mut chunked = data.clone();
        let mut offset = 0;
        for chunk in chunked.chunks_mut(37) {
            cipher.apply(chunk, offset);
            offset += chunk.len();
        }

        assert_eq!(single, chunked, "chunked stream must match single pass");
    }

    #[test]
    fn different_keys_produce_different_output() {
        let data = vec![0x42u8; 128];

        let mut a = data.clone();
        NcmStreamCipher::new(b"key-one").apply(&mut a, 0);

        let mut b = data.clone();
        NcmStreamCipher::new(b"key-two").apply(&mut b, 0);

        assert_ne!(a, b);
    }
}
