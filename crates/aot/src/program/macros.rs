use std::sync::OnceLock;

use lazy_static::lazy_static;
use snarkvm::{
    console::network::{CanaryV0, MainnetV0, TestnetV0},
    synthesizer::Process,
};

lazy_static! {
    pub static ref PROCESS_MAINNET: OnceLock<Process<MainnetV0>> = Default::default();
    pub static ref PROCESS_TESTNET: OnceLock<Process<TestnetV0>> = Default::default();
    pub static ref PROCESS_CANARY: OnceLock<Process<CanaryV0>> = Default::default();
}

#[macro_export]
macro_rules! network_match {
    (
        $circuit_ty:ident & $network_ty:ident =
        $($circuit_id:ident & $network_id:ident => { $( $additional:stmt; )* } );+ ,
        $e:expr
    ) => {
        match N::ID {
            $(
                <snarkvm::console::network::$network_id as Network>::ID => {
                    type $circuit_ty = snarkvm::circuit::$circuit_id;
                    type $network_ty = snarkvm::console::network::$network_id;
                    $($additional);*
                    $e
                }
            )*
            _ => unreachable!(),
        }
    }
}

#[macro_export]
macro_rules! use_aleo_network {
    ($a:ident, $n:ident, $e: expr) => {
        $crate::network_match!(
            $a & $n = AleoV0 & MainnetV0 => {}; AleoTestnetV0 & TestnetV0 => {}; AleoCanaryV0 & CanaryV0 => {},
            $e
        )
    };
}

/// Use the process for the network and return a non-network related value
#[macro_export]
macro_rules! use_process {
    ($a:ident, $n:ident, |$process:ident| $e:expr) => {
        $crate::network_match!(
            $a & $n =
            AleoV0 & MainnetV0 => {
                let $process = $crate::program::PROCESS_MAINNET.get_or_init(|| snarkvm::synthesizer::Process::load().unwrap());
            };
            AleoTestnetV0 & TestnetV0 => {
                let $process =
                $crate::program::PROCESS_TESTNET.get_or_init(|| snarkvm::synthesizer::Process::load().unwrap());
            };
            AleoCanaryV0 & CanaryV0 => {
                let $process =
                $crate::program::PROCESS_CANARY.get_or_init(|| snarkvm::synthesizer::Process::load().unwrap());
            },
            $e
        )
    };
}

/// Use the process for the network and return a non-network related value
#[macro_export]
macro_rules! use_process_mut {
    ($a:ident, $n:ident, |$process:ident| $e:expr) => {

        $crate::network_match!(
            $a & $n =
            AleoV0 & MainnetV0 => {
                let mut $process = snarkvm::synthesizer::Process::<snarkvm::console::network::MainnetV0>::load().unwrap();
            };
            AleoTestnetV0 & TestnetV0 => {
                let mut $process = snarkvm::synthesizer::Process::<snarkvm::console::network::TestnetV0>::load().unwrap();
            };
            AleoCanaryV0 & CanaryV0 => {
                let mut $process = snarkvm::synthesizer::Process::<snarkvm::console::network::CanaryV0>::load().unwrap();
            },
            $e
        )
    };
}

/// Provide an Aleo and Network type based on the network ID, then return a
/// downcasted value back to the generic network...
#[macro_export]
macro_rules! use_aleo_network_downcast {
    ($a:ident, $n:ident, $e:expr) => {
        *($crate::use_aleo_network!($a, $n, (Box::new($e) as Box<dyn std::any::Any>)))
            .downcast::<_>()
            .expect("Failed to downcast")
    };
}

/// Use the process for the network, then return a downcasted value back to the
/// generic network...
#[macro_export]
macro_rules! use_process_downcast {
    ($a:ident, $n:ident, |$process:ident| $e:expr) => {
        *($crate::use_process!($a, $n, |$process| (Box::new($e) as Box<dyn std::any::Any>)))
            .downcast::<_>()
            .expect("Failed to downcast")
    };
}
