pub const MASK_PREFIX_LEN: usize = 5;

#[repr(usize)]
pub enum MaskBit {
    Validator = 0,
    Prover = 1,
    Client = 2,
    Compute = 3,
    LocalPrivateKey = 4,
}
