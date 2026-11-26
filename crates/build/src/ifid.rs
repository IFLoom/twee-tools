//! IFIDs: a version 4 UUID in capitals, as Twine makes and the Twee 3 spec requires.

/// A new IFID.
pub fn new() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the system's random numbers");
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

#[cfg(test)]
mod tests {
    #[test]
    fn makes_v4_uuids_in_capitals() {
        let a = super::new();
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
        assert!(matches!(&a[19..20], "8" | "9" | "A" | "B"));
        assert_eq!(a, a.to_uppercase());
        assert_ne!(a, super::new());
    }
}
