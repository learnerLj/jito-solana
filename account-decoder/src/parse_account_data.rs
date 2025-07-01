/// Re-export parsed account types for client consumption
pub use solana_account_decoder_client_types::ParsedAccount;

use {
    // Import all specialized parsers for different account types
    crate::{
        parse_address_lookup_table::parse_address_lookup_table,
        parse_bpf_loader::parse_bpf_upgradeable_loader, 
        parse_config::parse_config,
        parse_nonce::parse_nonce, 
        parse_stake::parse_stake, 
        parse_sysvar::parse_sysvar,
        parse_token::parse_token_v3, 
        parse_vote::parse_vote,
    },
    inflector::Inflector,                           // For converting enum names to kebab-case
    solana_clock::UnixTimestamp,
    solana_instruction::error::InstructionError,
    solana_pubkey::Pubkey,
    // Solana program IDs for identifying account types by owner
    solana_sdk_ids::{
        address_lookup_table, bpf_loader_upgradeable, config, stake, system_program, sysvar, vote,
    },
    // SPL Token 2022 extension types for additional parsing context
    spl_token_2022::extension::{
        interest_bearing_mint::InterestBearingConfig, 
        scaled_ui_amount::ScaledUiAmountConfig,
    },
    std::collections::HashMap,
    thiserror::Error,
};

/// Global registry mapping program IDs to their parsable account types
/// 
/// This static map enables the account decoder to identify which parser
/// to use for a given account based on its owner program ID. The map is
/// lazily initialized and cached for efficient lookup.
pub static PARSABLE_PROGRAM_IDS: std::sync::LazyLock<HashMap<Pubkey, ParsableAccount>> =
    std::sync::LazyLock::new(|| {
        let mut m = HashMap::new();
        
        // Address Lookup Tables for transaction compression
        m.insert(
            address_lookup_table::id(),
            ParsableAccount::AddressLookupTable,
        );
        
        // BPF upgradeable loader for smart contracts
        m.insert(
            bpf_loader_upgradeable::id(),
            ParsableAccount::BpfUpgradeableLoader,
        );
        
        // Configuration accounts (deprecated)
        m.insert(config::id(), ParsableAccount::Config);
        
        // System program handles nonce accounts among others
        m.insert(system_program::id(), ParsableAccount::Nonce);
        
        // SPL Token (original) and Token-2022 programs
        m.insert(spl_token::id(), ParsableAccount::SplToken);
        m.insert(spl_token_2022::id(), ParsableAccount::SplToken2022);
        
        // Stake program for Proof of Stake consensus
        m.insert(stake::id(), ParsableAccount::Stake);
        
        // System variables (sysvars) for cluster state
        m.insert(sysvar::id(), ParsableAccount::Sysvar);
        
        // Vote program for validator consensus
        m.insert(vote::id(), ParsableAccount::Vote);
        
        m
    });

/// Errors that can occur during account parsing
#[derive(Error, Debug)]
pub enum ParseAccountError {
    /// The account type is recognized but the specific data cannot be parsed
    #[error("{0:?} account not parsable")]
    AccountNotParsable(ParsableAccount),

    /// The program owner is not in the list of parsable programs
    #[error("Program not parsable")]
    ProgramNotParsable,

    /// Required additional context data is missing (e.g., token decimals)
    #[error("Additional data required to parse: {0}")]
    AdditionalDataMissing(String),

    /// Error occurred during account data deserialization
    #[error("Instruction error")]
    InstructionError(#[from] InstructionError),

    /// Error occurred during JSON serialization of parsed data
    #[error("Serde json error")]
    SerdeJsonError(#[from] serde_json::error::Error),
}

/// Enumeration of all supported parsable account types
/// 
/// This enum identifies the different types of Solana accounts that
/// the decoder can parse into structured formats. Each variant
/// corresponds to a specific program type.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParsableAccount {
    /// Address lookup tables for transaction compression
    AddressLookupTable,
    /// BPF upgradeable loader accounts for smart contracts
    BpfUpgradeableLoader,
    /// Configuration accounts (deprecated)
    Config,
    /// Nonce accounts for durable transactions
    Nonce,
    /// SPL Token (original) accounts
    SplToken,
    /// SPL Token-2022 accounts with extensions
    SplToken2022,
    /// Stake accounts for Proof of Stake
    Stake,
    /// System variables (cluster state)
    Sysvar,
    /// Vote accounts for validator consensus
    Vote,
}

#[deprecated(since = "2.0.0", note = "Use `AccountAdditionalDataV3` instead")]
#[derive(Clone, Copy, Default)]
pub struct AccountAdditionalData {
    pub spl_token_decimals: Option<u8>,
}

#[deprecated(since = "2.2.0", note = "Use `AccountAdditionalDataV3` instead")]
#[derive(Clone, Copy, Default)]
pub struct AccountAdditionalDataV2 {
    pub spl_token_additional_data: Option<SplTokenAdditionalData>,
}

#[derive(Clone, Copy, Default)]
pub struct AccountAdditionalDataV3 {
    pub spl_token_additional_data: Option<SplTokenAdditionalDataV2>,
}

#[allow(deprecated)]
impl From<AccountAdditionalDataV2> for AccountAdditionalDataV3 {
    fn from(v: AccountAdditionalDataV2) -> Self {
        Self {
            spl_token_additional_data: v.spl_token_additional_data.map(Into::into),
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct SplTokenAdditionalData {
    pub decimals: u8,
    pub interest_bearing_config: Option<(InterestBearingConfig, UnixTimestamp)>,
}

impl SplTokenAdditionalData {
    pub fn with_decimals(decimals: u8) -> Self {
        Self {
            decimals,
            ..Default::default()
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct SplTokenAdditionalDataV2 {
    pub decimals: u8,
    pub interest_bearing_config: Option<(InterestBearingConfig, UnixTimestamp)>,
    pub scaled_ui_amount_config: Option<(ScaledUiAmountConfig, UnixTimestamp)>,
}

impl From<SplTokenAdditionalData> for SplTokenAdditionalDataV2 {
    fn from(v: SplTokenAdditionalData) -> Self {
        Self {
            decimals: v.decimals,
            interest_bearing_config: v.interest_bearing_config,
            scaled_ui_amount_config: None,
        }
    }
}

impl SplTokenAdditionalDataV2 {
    pub fn with_decimals(decimals: u8) -> Self {
        Self {
            decimals,
            ..Default::default()
        }
    }
}

#[deprecated(since = "2.0.0", note = "Use `parse_account_data_v3` instead")]
#[allow(deprecated)]
pub fn parse_account_data(
    pubkey: &Pubkey,
    program_id: &Pubkey,
    data: &[u8],
    additional_data: Option<AccountAdditionalData>,
) -> Result<ParsedAccount, ParseAccountError> {
    parse_account_data_v3(
        pubkey,
        program_id,
        data,
        additional_data.map(|d| AccountAdditionalDataV3 {
            spl_token_additional_data: d
                .spl_token_decimals
                .map(SplTokenAdditionalDataV2::with_decimals),
        }),
    )
}

#[deprecated(since = "2.2.0", note = "Use `parse_account_data_v3` instead")]
#[allow(deprecated)]
pub fn parse_account_data_v2(
    pubkey: &Pubkey,
    program_id: &Pubkey,
    data: &[u8],
    additional_data: Option<AccountAdditionalDataV2>,
) -> Result<ParsedAccount, ParseAccountError> {
    parse_account_data_v3(pubkey, program_id, data, additional_data.map(Into::into))
}

/// Parse account data into a structured, UI-friendly format
/// 
/// This is the main entry point for converting raw account data into
/// a structured `ParsedAccount` that can be easily consumed by clients.
/// It identifies the account type based on the owner program and delegates
/// to the appropriate specialized parser.
/// 
/// # Arguments
/// * `pubkey` - The account's public key (address)
/// * `program_id` - The program that owns this account
/// * `data` - The raw account data to parse
/// * `additional_data` - Optional context data needed for certain parsers
/// 
/// # Returns
/// A `ParsedAccount` containing the structured account information
/// 
/// # Errors
/// Returns `ParseAccountError` if:
/// - The program is not in the list of parsable programs
/// - The account data cannot be parsed by the appropriate parser
/// - Required additional data is missing
pub fn parse_account_data_v3(
    pubkey: &Pubkey,
    program_id: &Pubkey,
    data: &[u8],
    additional_data: Option<AccountAdditionalDataV3>,
) -> Result<ParsedAccount, ParseAccountError> {
    // Look up the appropriate parser for this program type
    let program_name = PARSABLE_PROGRAM_IDS
        .get(program_id)
        .ok_or(ParseAccountError::ProgramNotParsable)?;
    
    let additional_data = additional_data.unwrap_or_default();
    
    // Route to the appropriate specialized parser based on program type
    let parsed_json = match program_name {
        ParsableAccount::AddressLookupTable => {
            serde_json::to_value(parse_address_lookup_table(data)?)?
        }
        ParsableAccount::BpfUpgradeableLoader => {
            serde_json::to_value(parse_bpf_upgradeable_loader(data)?)?
        }
        ParsableAccount::Config => serde_json::to_value(parse_config(data, pubkey)?)?,
        ParsableAccount::Nonce => serde_json::to_value(parse_nonce(data)?)?,
        // Both SPL Token variants use the same parser
        ParsableAccount::SplToken | ParsableAccount::SplToken2022 => serde_json::to_value(
            parse_token_v3(data, additional_data.spl_token_additional_data.as_ref())?,
        )?,
        ParsableAccount::Stake => serde_json::to_value(parse_stake(data)?)?,
        ParsableAccount::Sysvar => serde_json::to_value(parse_sysvar(data, pubkey)?)?,
        ParsableAccount::Vote => serde_json::to_value(parse_vote(data)?)?,
    };
    
    // Construct the final parsed account representation
    Ok(ParsedAccount {
        program: format!("{program_name:?}").to_kebab_case(), // Convert to kebab-case for UI
        parsed: parsed_json,                                   // The structured account data
        space: data.len() as u64,                             // Account data size
    })
}

#[cfg(test)]
mod test {
    use {
        super::*,
        solana_nonce::{
            state::{Data, State},
            versions::Versions,
        },
        solana_vote_interface::{
            program::id as vote_program_id,
            state::{VoteState, VoteStateVersions},
        },
    };

    #[test]
    fn test_parse_account_data() {
        let account_pubkey = solana_pubkey::new_rand();
        let other_program = solana_pubkey::new_rand();
        let data = vec![0; 4];
        assert!(parse_account_data_v3(&account_pubkey, &other_program, &data, None).is_err());

        let vote_state = VoteState::default();
        let mut vote_account_data: Vec<u8> = vec![0; VoteState::size_of()];
        let versioned = VoteStateVersions::new_current(vote_state);
        VoteState::serialize(&versioned, &mut vote_account_data).unwrap();
        let parsed = parse_account_data_v3(
            &account_pubkey,
            &vote_program_id(),
            &vote_account_data,
            None,
        )
        .unwrap();
        assert_eq!(parsed.program, "vote".to_string());
        assert_eq!(parsed.space, VoteState::size_of() as u64);

        let nonce_data = Versions::new(State::Initialized(Data::default()));
        let nonce_account_data = bincode::serialize(&nonce_data).unwrap();
        let parsed = parse_account_data_v3(
            &account_pubkey,
            &system_program::id(),
            &nonce_account_data,
            None,
        )
        .unwrap();
        assert_eq!(parsed.program, "nonce".to_string());
        assert_eq!(parsed.space, State::size() as u64);
    }
}
