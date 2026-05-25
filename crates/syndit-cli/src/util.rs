use rand_core::RngCore;

pub fn random_hex(len: usize) -> String {
    let mut buf = vec![0u8; len];
    rand_core::OsRng.fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}
