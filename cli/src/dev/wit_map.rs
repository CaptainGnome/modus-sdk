#[allow(dead_code)]
pub mod v21 {
    use crate::dev::host::abi_2_1::modus;
    include!("wit_map_body.rs");
}

#[allow(dead_code)]
pub mod v20 {
    use crate::dev::host::abi_2_0::modus;
    include!("wit_map_body.rs");
}

pub(crate) use v21::*;
