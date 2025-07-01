#![allow(clippy::arithmetic_side_effects)]

#[macro_use]
extern crate serde_derive;

/// Module for parsing account data into structured UI-friendly formats
/// This is the main entry point for account parsing functionality
pub mod parse_account_data;

/// Parser for Solana Address Lookup Table accounts
/// Handles lookup tables used for transaction address compression
pub mod parse_address_lookup_table;

/// Parser for BPF (Berkeley Packet Filter) upgradeable loader accounts
/// Manages smart contract deployment and upgrades
pub mod parse_bpf_loader;

/// Parser for configuration accounts (deprecated in newer versions)
/// Handles system configuration and validator info accounts
#[allow(deprecated)]
pub mod parse_config;

/// Parser for nonce accounts used in durable transactions
/// Enables offline transaction signing with replay protection
pub mod parse_nonce;

/// Parser for stake accounts used in Solana's Proof of Stake consensus
/// Handles stake delegation, rewards, and validator operations
pub mod parse_stake;

/// Parser for system variable (sysvar) accounts
/// Decodes cluster-wide state like clock, fees, rent, etc.
pub mod parse_sysvar;

/// Parser for SPL Token and SPL Token-2022 accounts
/// Handles token mints, token accounts, and multisig accounts
pub mod parse_token;

/// Parser for SPL Token-2022 extensions
/// Handles advanced token features like transfer fees, confidential transfers, etc.
pub mod parse_token_extension;

/// Parser for vote accounts used in validator consensus
/// Manages validator voting records and authorization
pub mod parse_vote;

/// Validator information structure definitions
/// Contains metadata about network validators
pub mod validator_info;

/// Re-export UI account types for client consumption
/// These types provide a standardized interface for displaying account data
pub use solana_account_decoder_client_types::{
    UiAccount, UiAccountData, UiAccountEncoding, UiDataSliceConfig,
};
use {
    crate::parse_account_data::{parse_account_data_v3, AccountAdditionalDataV3},
    base64::{prelude::BASE64_STANDARD, Engine},
    solana_account::ReadableAccount,
    solana_fee_calculator::FeeCalculator,
    solana_pubkey::Pubkey,
    std::io::Write,
};

/// Type alias for representing amounts as strings to preserve precision
/// Used to avoid floating point precision issues with large numbers
pub type StringAmount = String;

/// Type alias for representing decimal places as strings
pub type StringDecimals = String;

/// Maximum number of bytes that can be Base58 encoded safely
/// Larger data will return an error message instead of encoded data
pub const MAX_BASE58_BYTES: usize = 128;

/// Encodes account data as Base58 string with size validation
/// 
/// # Arguments
/// * `account` - The account containing data to encode
/// * `data_slice_config` - Optional configuration to slice data before encoding
/// 
/// # Returns
/// Base58 encoded string or error message if data is too large
fn encode_bs58<T: ReadableAccount>(
    account: &T,
    data_slice_config: Option<UiDataSliceConfig>,
) -> String {
    let slice = slice_data(account.data(), data_slice_config);
    if slice.len() <= MAX_BASE58_BYTES {
        bs58::encode(slice).into_string()
    } else {
        "error: data too large for bs58 encoding".to_string()
    }
}

/// Encodes a Solana account into a UI-friendly representation
/// 
/// This is the main entry point for converting raw account data into various
/// encoded formats suitable for client consumption (JSON-RPC, web interfaces, etc.)
/// 
/// # Arguments
/// * `pubkey` - The public key (address) of the account
/// * `account` - The account data to encode
/// * `encoding` - Desired encoding format (Binary, Base58, Base64, etc.)
/// * `additional_data` - Additional context needed for parsing (e.g., token decimals)
/// * `data_slice_config` - Optional configuration to slice account data
/// 
/// # Returns
/// A `UiAccount` struct containing the encoded account information
pub fn encode_ui_account<T: ReadableAccount>(
    pubkey: &Pubkey,
    account: &T,
    encoding: UiAccountEncoding,
    additional_data: Option<AccountAdditionalDataV3>,
    data_slice_config: Option<UiDataSliceConfig>,
) -> UiAccount {
    let space = account.data().len();
    
    // Encode the account data based on the requested encoding format
    let data = match encoding {
        // Legacy binary encoding using Base58
        UiAccountEncoding::Binary => {
            let data = encode_bs58(account, data_slice_config);
            UiAccountData::LegacyBinary(data)
        }
        // Standard Base58 encoding
        UiAccountEncoding::Base58 => {
            let data = encode_bs58(account, data_slice_config);
            UiAccountData::Binary(data, encoding)
        }
        // Base64 encoding - more compact than Base58
        UiAccountEncoding::Base64 => UiAccountData::Binary(
            BASE64_STANDARD.encode(slice_data(account.data(), data_slice_config)),
            encoding,
        ),
        // Base64 with Zstandard compression for large data
        UiAccountEncoding::Base64Zstd => {
            let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 0).unwrap();
            match encoder
                .write_all(slice_data(account.data(), data_slice_config))
                .and_then(|()| encoder.finish())
            {
                // Successfully compressed - return compressed Base64
                Ok(zstd_data) => UiAccountData::Binary(BASE64_STANDARD.encode(zstd_data), encoding),
                // Compression failed - fallback to regular Base64
                Err(_) => UiAccountData::Binary(
                    BASE64_STANDARD.encode(slice_data(account.data(), data_slice_config)),
                    UiAccountEncoding::Base64,
                ),
            }
        }
        // Parse account data into structured JSON format
        UiAccountEncoding::JsonParsed => {
            if let Ok(parsed_data) =
                parse_account_data_v3(pubkey, account.owner(), account.data(), additional_data)
            {
                UiAccountData::Json(parsed_data)
            } else {
                // Parsing failed - fallback to Base64 encoding
                UiAccountData::Binary(
                    BASE64_STANDARD.encode(slice_data(account.data(), data_slice_config)),
                    UiAccountEncoding::Base64,
                )
            }
        }
    };
    
    // Construct the final UI account representation
    UiAccount {
        lamports: account.lamports(),              // Account balance in lamports
        data,                                      // Encoded account data 
        owner: account.owner().to_string(),        // Program that owns this account
        executable: account.executable(),          // Whether account contains executable code
        rent_epoch: account.rent_epoch(),          // Epoch when rent was last collected
        space: Some(space as u64),                 // Size of account data in bytes
    }
}

/// UI representation of fee calculator for transaction fees
/// Note: Fee calculators are deprecated in newer Solana versions
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UiFeeCalculator {
    /// Cost in lamports for each transaction signature
    pub lamports_per_signature: StringAmount,
}

impl From<FeeCalculator> for UiFeeCalculator {
    fn from(fee_calculator: FeeCalculator) -> Self {
        Self {
            lamports_per_signature: fee_calculator.lamports_per_signature.to_string(),
        }
    }
}

impl Default for UiFeeCalculator {
    fn default() -> Self {
        Self {
            lamports_per_signature: "0".to_string(),
        }
    }
}

/// Slices account data based on provided configuration
/// 
/// This function safely extracts a portion of account data, handling
/// boundary conditions to prevent panics.
/// 
/// # Arguments
/// * `data` - The raw account data to slice
/// * `data_slice_config` - Optional slice configuration (offset and length)
/// 
/// # Returns
/// A slice of the data, or empty slice if offset is out of bounds
fn slice_data(data: &[u8], data_slice_config: Option<UiDataSliceConfig>) -> &[u8] {
    if let Some(UiDataSliceConfig { offset, length }) = data_slice_config {
        if offset >= data.len() {
            // Offset beyond data length - return empty slice
            &[]
        } else if length > data.len() - offset {
            // Length extends beyond data - return from offset to end
            &data[offset..]
        } else {
            // Valid slice parameters - return exact slice
            &data[offset..offset + length]
        }
    } else {
        // No slicing requested - return full data
        data
    }
}

#[cfg(test)]
mod test {
    use {
        super::*,
        assert_matches::assert_matches,
        solana_account::{Account, AccountSharedData},
    };

    #[test]
    fn test_slice_data() {
        let data = vec![1, 2, 3, 4, 5];
        let slice_config = Some(UiDataSliceConfig {
            offset: 0,
            length: 5,
        });
        assert_eq!(slice_data(&data, slice_config), &data[..]);

        let slice_config = Some(UiDataSliceConfig {
            offset: 0,
            length: 10,
        });
        assert_eq!(slice_data(&data, slice_config), &data[..]);

        let slice_config = Some(UiDataSliceConfig {
            offset: 1,
            length: 2,
        });
        assert_eq!(slice_data(&data, slice_config), &data[1..3]);

        let slice_config = Some(UiDataSliceConfig {
            offset: 10,
            length: 2,
        });
        assert_eq!(slice_data(&data, slice_config), &[] as &[u8]);
    }

    #[test]
    fn test_encode_account_when_data_exceeds_base58_byte_limit() {
        let data = vec![42; MAX_BASE58_BYTES + 2];
        let account = AccountSharedData::from(Account {
            data,
            ..Account::default()
        });

        // Whole account
        assert_eq!(
            encode_bs58(&account, None),
            "error: data too large for bs58 encoding"
        );

        // Slice of account that's still too large
        assert_eq!(
            encode_bs58(
                &account,
                Some(UiDataSliceConfig {
                    length: MAX_BASE58_BYTES + 1,
                    offset: 1
                })
            ),
            "error: data too large for bs58 encoding"
        );

        // Slice of account that fits inside `MAX_BASE58_BYTES`
        assert_ne!(
            encode_bs58(
                &account,
                Some(UiDataSliceConfig {
                    length: MAX_BASE58_BYTES,
                    offset: 1
                })
            ),
            "error: data too large for bs58 encoding"
        );

        // Slice of account that's too large, but whose intersection with the account still fits
        assert_ne!(
            encode_bs58(
                &account,
                Some(UiDataSliceConfig {
                    length: MAX_BASE58_BYTES + 1,
                    offset: 2
                })
            ),
            "error: data too large for bs58 encoding"
        );
    }

    #[test]
    fn test_base64_zstd() {
        let encoded_account = encode_ui_account(
            &Pubkey::default(),
            &AccountSharedData::from(Account {
                data: vec![0; 1024],
                ..Account::default()
            }),
            UiAccountEncoding::Base64Zstd,
            None,
            None,
        );
        assert_matches!(
            encoded_account.data,
            UiAccountData::Binary(_, UiAccountEncoding::Base64Zstd)
        );

        let decoded_account = encoded_account.decode::<Account>().unwrap();
        assert_eq!(decoded_account.data(), &vec![0; 1024]);
        let decoded_account = encoded_account.decode::<AccountSharedData>().unwrap();
        assert_eq!(decoded_account.data(), &vec![0; 1024]);
    }
}
