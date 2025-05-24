use anchor_lang::prelude::*;
use anchor_lang::solana_program::{self, clock::Clock};
use anchor_spl::token::{self, Token, TokenAccount};
use anchor_spl::associated_token::AssociatedToken;
use chainlink_solana as chainlink;
use solana_program::program_pack::Pack;
#[cfg(not(feature = "no-entrypoint"))]
use {solana_security_txt::security_txt};

declare_id!("2wFmCLVQ8pSF2aKu43gLv2vzasUHhtmAA9HffBDXcRfF");

#[cfg(not(feature = "no-entrypoint"))]
security_txt! {
    name: "Referral Matrix System",
    project_url: "https://matrix.matrix",
    contacts: "email:01010101@matrix.io,discord:01010101,whatsapp:+55123456789",
    policy: "https://github.com/ghost-ai91/matrixv1.0-Beta/blob/main/SECURITY.md",
    preferred_languages: "en",
    source_code: "https://github.com/ghost-ai91/matrixv1.0-Beta/blob/main/programs/matrix-system/src/lib.rs",
    source_revision: env!("GITHUB_SHA", "unknown-revision"),
    source_release: env!("PROGRAM_VERSION", "unknown-version"),
    encryption: "",
    auditors: "",
    acknowledgements: "We thank all security researchers who contributed to the security of our protocol."
}

// Minimum deposit amount in USD (10 dollars in base units - 8 decimals)
const MINIMUM_USD_DEPOSIT: u64 = 10_00000000;

// Maximum price feed staleness (24 hours in seconds)
const MAX_PRICE_FEED_AGE: i64 = 86400;

// Default SOL price in case of stale feed ($100 USD per SOL)
const DEFAULT_SOL_PRICE: i128 = 100_00000000;

// Maximum number of upline accounts that can be processed in a single transaction
const MAX_UPLINE_DEPTH: usize = 6;

// Number of Vault A accounts in the remaining_accounts
const VAULT_A_ACCOUNTS_COUNT: usize = 3;

// Posições específicas no remaining_accounts
const CHAINLINK_ACCOUNTS_COUNT: usize = 2;
const WSOL_ACCOUNT_POSITION: usize = VAULT_A_ACCOUNTS_COUNT + CHAINLINK_ACCOUNTS_COUNT; // Posição 5

// Constants for strict address verification
pub mod verified_addresses {
    use solana_program::pubkey::Pubkey;

    pub static A_VAULT_LP: Pubkey = solana_program::pubkey!("BGh2tc4kagmEmVvaogdcAodVDvUxmXWivYL5kxwapm31");
    pub static A_VAULT_LP_MINT: Pubkey = solana_program::pubkey!("Bk33KwVZ8hsgr3uSb8GGNJZpAEqH488oYPvoY5W9djVP");
    pub static A_TOKEN_VAULT: Pubkey = solana_program::pubkey!("HoASBFustFYysd9aCu6M3G3kve88j22LAyTpvCNp5J65");
    
    pub static POOL_ADDRESS: Pubkey = solana_program::pubkey!("BEuzx33ecm4rtgjtB2bShqGco4zMkdr6ioyzPh6vY9ot");
    pub static B_VAULT_LP: Pubkey = solana_program::pubkey!("8mNjx5Aww9DX33uFxZwqb7m2vhsavrxyzkME3hE63sT2");
    
    pub static TOKEN_MINT: Pubkey = solana_program::pubkey!("3dCXCZd3cbKHT7jQSLzRNJQYu1zEzaD8FHi4MWHLX4DZ");
    pub static WSOL_MINT: Pubkey = solana_program::pubkey!("So11111111111111111111111111111111111111112");
    
    pub static CHAINLINK_PROGRAM: Pubkey = solana_program::pubkey!("HEvSKofvBgfaexv23kMabbYqxasxU3mQ4ibBMEmJWHny");
    pub static SOL_USD_FEED: Pubkey = solana_program::pubkey!("99B2bTijsU6f1GCT73HmdR7HCFFjGMBcPZY6jZ96ynrR");
}

pub mod admin_addresses {
    use solana_program::pubkey::Pubkey;

    pub static MULTISIG_TREASURY: Pubkey = solana_program::pubkey!("Eu22Js2qTu5bCr2WFY2APbvhDqAhUZpkYKmVsfeyqR2N");
    pub static AUTHORIZED_INITIALIZER: Pubkey = solana_program::pubkey!("8gVApS2cyCuYsGk7VqjMhTc6cSEBx6fhGz7T7wSrWEpv");
}

#[account]
pub struct ProgramState {
    pub owner: Pubkey,
    pub multisig_treasury: Pubkey,
    pub next_upline_id: u32,
    pub next_chain_id: u32,
    pub last_mint_amount: u64,
}

impl ProgramState {
    pub const SIZE: usize = 32 + 32 + 4 + 4 + 8;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default, Debug)]
pub struct UplineEntry {
    pub pda: Pubkey,
    pub wallet: Pubkey,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct ReferralUpline {
    pub id: u32,
    pub depth: u8,
    pub upline: Vec<UplineEntry>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct ReferralChain {
    pub id: u32,
    pub slots: [Option<Pubkey>; 3],
    pub filled_slots: u8,
}

#[account]
#[derive(Default)]
pub struct UserAccount {
    pub is_registered: bool,
    pub referrer: Option<Pubkey>,
    pub owner_wallet: Pubkey,
    pub upline: ReferralUpline,
    pub chain: ReferralChain,
    pub reserved_sol: u64,
    pub reserved_tokens: u64,
}

impl UserAccount {
    pub const SIZE: usize = 1 + 1 + 32 + 32 + 4 + 1 + 4 + (MAX_UPLINE_DEPTH * (32 + 32)) + 4 + (3 * (1 + 32)) + 1 + 8 + 8;
}

#[error_code]
pub enum ErrorCode {
    #[msg("State account already initialized")]
    AlreadyInitialized,
    #[msg("Invalid state account (must be owned by program)")]
    InvalidStateAccount,
    #[msg("Invalid state account size")]
    InvalidStateSize,
    #[msg("Invalid vault A LP address")]
    InvalidVaultALpAddress,
    #[msg("Invalid vault A LP mint address")]
    InvalidVaultALpMintAddress,
    #[msg("Invalid token A vault address")]
    InvalidTokenAVaultAddress,
    #[msg("Referrer account is not registered")]
    ReferrerNotRegistered,
    #[msg("Invalid upline relationship")]
    InvalidUpline,
    #[msg("Invalid upline depth")]
    InvalidUplineDepth,
    #[msg("Not authorized")]
    NotAuthorized,
    #[msg("Chain is already full")]
    ChainFull,
    #[msg("Slot account not owned by program")]
    InvalidSlotOwner,
    #[msg("Slot account not registered")]
    SlotNotRegistered,
    #[msg("Invalid referrer in chain slot")]
    InvalidSlotReferrer,
    #[msg("Cannot load upline account")]
    CannotLoadUplineAccount,
    #[msg("Invalid account discriminator")]
    InvalidAccountDiscriminator,
    #[msg("Insufficient deposit amount")]
    InsufficientDeposit,
    #[msg("Failed to process deposit to pool")]
    DepositToPoolFailed,
    #[msg("Failed to process SOL reserve")]
    SolReserveFailed,
    #[msg("Failed to process referrer payment")]
    ReferrerPaymentFailed,
    #[msg("Failed to wrap SOL to WSOL")]
    WrapSolFailed,
    #[msg("Failed to unwrap WSOL to SOL")]
    UnwrapSolFailed,
    #[msg("Failed to mint tokens")]
    TokenMintFailed,
    #[msg("Failed to transfer tokens")]
    TokenTransferFailed,
    #[msg("Invalid pool address")]
    InvalidPoolAddress,
    #[msg("Invalid vault address")]
    InvalidVaultAddress,
    #[msg("Invalid token mint address")]
    InvalidTokenMintAddress,
    #[msg("Invalid token account")]
    InvalidTokenAccount,
    #[msg("Invalid wallet for ATA")]
    InvalidWalletForATA,
    #[msg("Failed to create upline entry")]
    UplineEntryCreationFailed,
    #[msg("Missing required account for upline")]
    MissingUplineAccount,
    #[msg("Payment wallet is not a system account")]
    PaymentWalletInvalid,
    #[msg("Token account is not a valid ATA")]
    TokenAccountInvalid,
    #[msg("Missing vault A accounts")]
    MissingVaultAAccounts,
    #[msg("Failed to read price feed")]
    PriceFeedReadFailed,
    #[msg("Price feed too old")]
    PriceFeedTooOld,
    #[msg("Invalid Chainlink program")]
    InvalidChainlinkProgram,
    #[msg("Invalid price feed")]
    InvalidPriceFeed,
    #[msg("WSOL account not provided when required")]
    MissingWsolAccount,
    #[msg("Invalid WSOL account")]
    InvalidWsolAccount,
}

#[event]
pub struct SlotFilled {
    pub slot_idx: u8,
    pub chain_id: u32,
    pub user: Pubkey,
    pub owner: Pubkey,
}

#[derive(Default)]
pub struct Decimal {
    pub value: i128,
    pub decimals: u32,
}

impl Decimal {
    pub fn new(value: i128, decimals: u32) -> Self {
        Decimal { value, decimals }
    }
}

impl std::fmt::Display for Decimal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut scaled_val = self.value.to_string();
        if scaled_val.len() <= self.decimals as usize {
            scaled_val.insert_str(
                0,
                &vec!["0"; self.decimals as usize - scaled_val.len()].join(""),
            );
            scaled_val.insert_str(0, "0.");
        } else {
            scaled_val.insert(scaled_val.len() - self.decimals as usize, '.');
        }
        f.write_str(&scaled_val)
    }
}

fn force_memory_cleanup() {
    let _dummy = Vec::<u8>::new();
}

fn get_sol_usd_price<'info>(
    chainlink_feed: &AccountInfo<'info>,
    chainlink_program: &AccountInfo<'info>,
) -> Result<(i128, u32, i64, i64)> {
    let round = chainlink::latest_round_data(
        chainlink_program.clone(),
        chainlink_feed.clone(),
    ).map_err(|_| error!(ErrorCode::PriceFeedReadFailed))?;

    let decimals = chainlink::decimals(
        chainlink_program.clone(),
        chainlink_feed.clone(),
    ).map_err(|_| error!(ErrorCode::PriceFeedReadFailed))?;

    let clock = Clock::get()?;
    let current_timestamp = clock.unix_timestamp;
    
    Ok((round.answer, decimals.into(), current_timestamp, round.timestamp.into()))
}

fn calculate_minimum_sol_deposit<'info>(
    chainlink_feed: &AccountInfo<'info>, 
    chainlink_program: &AccountInfo<'info>
) -> Result<u64> {
    let (price, decimals, current_timestamp, feed_timestamp) = get_sol_usd_price(chainlink_feed, chainlink_program)?;
    
    let age = current_timestamp - feed_timestamp;
    
    let sol_price_per_unit = if age > MAX_PRICE_FEED_AGE {
        DEFAULT_SOL_PRICE
    } else {
        price
    };
    
    let price_f64 = sol_price_per_unit as f64 / 10f64.powf(decimals as f64);
    let minimum_usd_f64 = MINIMUM_USD_DEPOSIT as f64 / 1_00000000.0;
    let minimum_sol_f64 = minimum_usd_f64 / price_f64;
    let minimum_lamports = (minimum_sol_f64 * 1_000_000_000.0) as u64;
    
    Ok(minimum_lamports)
}

fn check_mint_limit(program_state: &mut ProgramState, proposed_mint_value: u64) -> Result<u64> {
    if program_state.last_mint_amount == 0 {
        msg!("First mint: establishing base value for limiter: {}", proposed_mint_value);
        program_state.last_mint_amount = proposed_mint_value;
        return Ok(proposed_mint_value);
    }
    
    let current_limit = program_state.last_mint_amount.saturating_mul(3);
    
    if proposed_mint_value > current_limit {
        msg!(
            "Mint adjustment: {} exceeds limit of {} (3x last mint). Using previous value: {}",
            proposed_mint_value,
            current_limit,
            program_state.last_mint_amount
        );
        return Ok(program_state.last_mint_amount);
    }
    
    program_state.last_mint_amount = proposed_mint_value;
    Ok(proposed_mint_value)
}

fn get_donut_tokens_amount<'info>(
    a_vault_lp: &AccountInfo<'info>,
    b_vault_lp: &AccountInfo<'info>,
    a_vault_lp_mint: &AccountInfo<'info>,
    b_vault_lp_mint: &AccountInfo<'info>,
    a_token_vault: &AccountInfo<'info>,
    b_token_vault: &AccountInfo<'info>,
    sol_amount: u64,
) -> Result<u64> {
    const METEORA_FEE: i128 = 1800;
    const FEE_DENOMINATOR: i128 = 10000;
    const PRECISION_FACTOR: i128 = 1_000_000_000;
    
    msg!("get_donut_tokens_amount called with sol_amount: {}", sol_amount);
    
    let a_vault_lp_amount: u64;
    let b_vault_lp_amount: u64;
    
    {
        let a_vault_lp_data = match spl_token::state::Account::unpack(&a_vault_lp.try_borrow_data()?) {
            Ok(data) => data,
            Err(_) => {
                msg!("Error reading LP data");
                return Ok(100);
            }
        };
            
        let b_vault_lp_data = match spl_token::state::Account::unpack(&b_vault_lp.try_borrow_data()?) {
            Ok(data) => data,
            Err(_) => {
                msg!("Error reading LP data");
                return Ok(100);
            }
        };
        
        a_vault_lp_amount = a_vault_lp_data.amount;
        b_vault_lp_amount = b_vault_lp_data.amount;
        msg!("LP amounts - A: {}, B: {}", a_vault_lp_amount, b_vault_lp_amount);
    }
    
    force_memory_cleanup();
    
    let a_vault_lp_supply: u64;
    let b_vault_lp_supply: u64;
    
    {
        let a_vault_lp_mint_data = match spl_token::state::Mint::unpack(&a_vault_lp_mint.try_borrow_data()?) {
            Ok(data) => data,
            Err(_) => {
                msg!("Error reading LP mint data");
                return Ok(100);
            }
        };
            
        let b_vault_lp_mint_data = match spl_token::state::Mint::unpack(&b_vault_lp_mint.try_borrow_data()?) {
            Ok(data) => data,
            Err(_) => {
                msg!("Error reading LP mint data");
                return Ok(100);
            }
        };
        
        a_vault_lp_supply = a_vault_lp_mint_data.supply;
        b_vault_lp_supply = b_vault_lp_mint_data.supply;
        msg!("LP supplies - A: {}, B: {}", a_vault_lp_supply, b_vault_lp_supply);
    }
    
    force_memory_cleanup();
    
    let total_token_a_amount: u64;
    let total_token_b_amount: u64;
    
    {
        let a_token_vault_data = match spl_token::state::Account::unpack(&a_token_vault.try_borrow_data()?) {
            Ok(data) => data,
            Err(_) => {
                msg!("Error reading token vault data");
                return Ok(100);
            }
        };
            
        let b_token_vault_data = match spl_token::state::Account::unpack(&b_token_vault.try_borrow_data()?) {
            Ok(data) => data,
            Err(_) => {
                msg!("Error reading token vault data");
                return Ok(100);
            }
        };
        
        total_token_a_amount = a_token_vault_data.amount;
        total_token_b_amount = b_token_vault_data.amount;
        msg!("Total token amounts - A: {}, B: {}", total_token_a_amount, total_token_b_amount);
    }
    
    force_memory_cleanup();
    
    if a_vault_lp_supply == 0 || b_vault_lp_supply == 0 || total_token_a_amount == 0 || total_token_b_amount == 0 {
        msg!("Zero values detected, using fallback");
        return Ok(100);
    }
    
    let a_lp_amount_big = a_vault_lp_amount as i128;
    let a_lp_supply_big = a_vault_lp_supply as i128;
    let b_lp_amount_big = b_vault_lp_amount as i128;
    let b_lp_supply_big = b_vault_lp_supply as i128;
    let token_a_big = total_token_a_amount as i128;
    let token_b_big = total_token_b_amount as i128;
    let sol_amount_big = sol_amount as i128;
    
    let pool_token_a = match token_a_big.checked_mul(a_lp_amount_big) {
        Some(num) => match num.checked_div(a_lp_supply_big) {
            Some(result) => result,
            None => {
                msg!("Division by zero in pool_token_a calculation");
                return Ok(100);
            }
        },
        None => {
            msg!("Overflow in pool_token_a calculation");
            return Ok(100);
        }
    };
    
    let pool_token_b = match token_b_big.checked_mul(b_lp_amount_big) {
        Some(num) => match num.checked_div(b_lp_supply_big) {
            Some(result) => result,
            None => {
                msg!("Division by zero in pool_token_b calculation");
                return Ok(100);
            }
        },
        None => {
            msg!("Overflow in pool_token_b calculation");
            return Ok(100);
        }
    };
    
    msg!("Pool tokens - A: {}, B: {}", pool_token_a, pool_token_b);
    
    if pool_token_a == 0 || pool_token_b == 0 {
        msg!("Zero pool tokens, using fallback");
        return Ok(100);
    }
    
    let basic_ratio = match pool_token_a.checked_mul(PRECISION_FACTOR) {
        Some(num) => match num.checked_div(pool_token_b) {
            Some(result) => result,
            None => {
                msg!("Division by zero in basic_ratio calculation");
                return Ok(100);
            }
        },
        None => {
            msg!("Overflow in basic_ratio calculation");
            return Ok(100);
        }
    };
    
    msg!("Basic ratio without fees (scaled): {}", basic_ratio);
    
    let fee_multiplier = match FEE_DENOMINATOR.checked_mul(PRECISION_FACTOR) {
        Some(num) => {
            let denominator = match FEE_DENOMINATOR.checked_sub(METEORA_FEE) {
                Some(val) => val,
                None => {
                    msg!("Underflow in fee denominator calculation");
                    return Ok(100);
                }
            };
            
            match num.checked_div(denominator) {
                Some(result) => result,
                None => {
                    msg!("Division by zero in fee_multiplier calculation");
                    return Ok(100);
                }
            }
        },
        None => {
            msg!("Overflow in fee_multiplier calculation");
            return Ok(100);
        }
    };
    
    msg!("Fee multiplier (scaled): {}", fee_multiplier);
    
    let fee_adjusted_ratio = match basic_ratio.checked_mul(fee_multiplier) {
        Some(num) => match num.checked_div(PRECISION_FACTOR) {
            Some(result) => result,
            None => {
                msg!("Division by zero in fee_adjusted_ratio calculation");
                return Ok(100);
            }
        },
        None => {
            msg!("Overflow in fee_adjusted_ratio calculation");
            return Ok(100);
        }
    };
    
    msg!("Fee adjusted ratio (scaled): {}", fee_adjusted_ratio);
    
    let donut_tokens_scaled = match sol_amount_big.checked_mul(fee_adjusted_ratio) {
        Some(result) => result,
        None => {
            msg!("Overflow in donut_tokens_scaled calculation");
            return Ok(100);
        }
    };
    
    msg!("Donut tokens scaled: {}", donut_tokens_scaled);
    
    let donut_tokens_big = match donut_tokens_scaled.checked_div(PRECISION_FACTOR) {
        Some(result) => result,
        None => {
            msg!("Division by zero in donut_tokens_big calculation");
            return Ok(100);
        }
    };

    msg!("donut_tokens_big (i128): {}", donut_tokens_big);
    
    if donut_tokens_big > i128::from(u64::MAX) {
        msg!("donut_tokens_big exceeds u64::MAX");
        return Ok(100);
    }

    let donut_tokens = donut_tokens_big as u64;

    msg!("Final donut_tokens (u64): {}", donut_tokens);
    
    if donut_tokens == 0 {
        if donut_tokens_big > 0 {
            msg!("Small positive value truncated to zero, returning minimum value");
            return Ok(1);
        }
        
        msg!("donut_tokens is zero, using fallback");
        return Ok(100);
    }
    
    Ok(donut_tokens)
}

fn verify_address_strict(provided: &Pubkey, expected: &Pubkey, error_code: ErrorCode) -> Result<()> {
    if provided != expected {
        return Err(error!(error_code));
    }
    Ok(())
}

fn verify_vault_a_addresses<'info>(
    a_vault_lp: &Pubkey,
    a_vault_lp_mint: &Pubkey,
    a_token_vault: &Pubkey
) -> Result<()> {
    verify_address_strict(a_vault_lp, &verified_addresses::A_VAULT_LP, ErrorCode::InvalidVaultALpAddress)?;
    verify_address_strict(a_vault_lp_mint, &verified_addresses::A_VAULT_LP_MINT, ErrorCode::InvalidVaultALpMintAddress)?;
    verify_address_strict(a_token_vault, &verified_addresses::A_TOKEN_VAULT, ErrorCode::InvalidTokenAVaultAddress)?;
    
    Ok(())
}

fn verify_ata_strict<'info>(
    token_account: &AccountInfo<'info>,
    owner: &Pubkey,
    expected_mint: &Pubkey
) -> Result<()> {
    if token_account.owner != &spl_token::id() {
        return Err(error!(ErrorCode::InvalidTokenAccount));
    }
    
    match TokenAccount::try_deserialize(&mut &token_account.data.borrow()[..]) {
        Ok(token_data) => {
            if token_data.owner != *owner {
                return Err(error!(ErrorCode::InvalidWalletForATA));
            }
            
            if token_data.mint != *expected_mint {
                return Err(error!(ErrorCode::InvalidTokenMintAddress));
            }
        },
        Err(_) => {
            return Err(error!(ErrorCode::InvalidTokenAccount));
        }
    }
    
    Ok(())
}

fn verify_all_fixed_addresses<'info>(
    pool: &Pubkey,
    b_vault_lp: &Pubkey,
    token_mint: &Pubkey,
    wsol_mint: &Pubkey,
) -> Result<()> {
    verify_address_strict(pool, &verified_addresses::POOL_ADDRESS, ErrorCode::InvalidPoolAddress)?;
    verify_address_strict(b_vault_lp, &verified_addresses::B_VAULT_LP, ErrorCode::InvalidVaultAddress)?;
    verify_address_strict(token_mint, &verified_addresses::TOKEN_MINT, ErrorCode::InvalidTokenMintAddress)?;
    verify_address_strict(wsol_mint, &verified_addresses::WSOL_MINT, ErrorCode::InvalidTokenMintAddress)?;
    
    Ok(())
}

fn verify_chainlink_addresses<'info>(
    chainlink_program: &Pubkey,
    chainlink_feed: &Pubkey,
) -> Result<()> {
    verify_address_strict(chainlink_program, &verified_addresses::CHAINLINK_PROGRAM, ErrorCode::InvalidChainlinkProgram)?;
    verify_address_strict(chainlink_feed, &verified_addresses::SOL_USD_FEED, ErrorCode::InvalidPriceFeed)?;
    
    Ok(())
}

fn verify_wallet_is_system_account<'info>(wallet: &AccountInfo<'info>) -> Result<()> {
    if wallet.owner != &solana_program::system_program::ID {
        return Err(error!(ErrorCode::PaymentWalletInvalid));
    }
    
    Ok(())
}

fn verify_token_account<'info>(
    token_account: &AccountInfo<'info>,
    wallet: &Pubkey,
    token_mint: &Pubkey
) -> Result<()> {
    if token_account.owner != &spl_token::id() {
        return Err(error!(ErrorCode::TokenAccountInvalid));
    }
    
    let token_data = match TokenAccount::try_deserialize(&mut &token_account.data.borrow()[..]) {
        Ok(data) => data,
        Err(_) => {
            return Err(error!(ErrorCode::TokenAccountInvalid));
        }
    };
    
    if token_data.owner != *wallet {
        return Err(error!(ErrorCode::TokenAccountInvalid));
    }
    
    if token_data.mint != *token_mint {
        return Err(error!(ErrorCode::TokenAccountInvalid));
    }
    
    Ok(())
}

// 🎯 FUNÇÃO CHAVE: Verificar e validar WSOL via remaining_accounts
fn verify_wsol_account<'info>(
    wsol_account: &AccountInfo<'info>,
    expected_owner: &Pubkey,
    wsol_mint: &Pubkey,
) -> Result<()> {
    msg!("🔍 Verificando conta WSOL: {}", wsol_account.key());
    
    // Verificar se é uma conta de token válida
    if wsol_account.owner != &spl_token::id() {
        msg!("❌ WSOL account não é uma conta de token válida");
        return Err(error!(ErrorCode::InvalidTokenAccount));
    }
    
    // Deserializar e verificar dados da conta
    match TokenAccount::try_deserialize(&mut &wsol_account.data.borrow()[..]) {
        Ok(token_data) => {
            if token_data.owner != *expected_owner {
                msg!("❌ WSOL account owner mismatch. Expected: {}, Found: {}", expected_owner, token_data.owner);
                return Err(error!(ErrorCode::InvalidWalletForATA));
            }
            
            if token_data.mint != *wsol_mint {
                msg!("❌ WSOL account mint mismatch. Expected: {}, Found: {}", wsol_mint, token_data.mint);
                return Err(error!(ErrorCode::InvalidTokenMintAddress));
            }
            
            msg!("✅ WSOL account verificada com sucesso: {} (balance: {})", wsol_account.key(), token_data.amount);
        },
        Err(e) => {
            msg!("❌ Erro ao deserializar WSOL account: {:?}", e);
            return Err(error!(ErrorCode::InvalidTokenAccount));
        }
    }
    
    Ok(())
}

// 🚀 FUNÇÃO PRINCIPAL: Processar depósito usando WSOL via remaining_accounts
fn process_deposit_with_wsol_from_remaining<'info>(
    user_wallet: &Signer<'info>,
    wsol_account: &AccountInfo<'info>,
    wsol_mint: &AccountInfo<'info>,
    b_vault_lp: &AccountInfo<'info>,
    b_vault: &UncheckedAccount<'info>,
    b_token_vault: &AccountInfo<'info>,
    b_vault_lp_mint: &AccountInfo<'info>,
    vault_program: &UncheckedAccount<'info>,
    token_program: &Program<'info, Token>,
    amount: u64,
) -> Result<()> {
    msg!("🎯 PROCESSANDO DEPÓSITO: {} lamports usando WSOL from remaining_accounts", amount);
    
    // 1. Verificar se a conta WSOL é válida
    verify_wsol_account(wsol_account, &user_wallet.key(), &wsol_mint.key())?;
    
    // 2. Usar a função existente process_deposit_to_pool com WSOL
    process_deposit_to_pool(
        &user_wallet.to_account_info(),
        wsol_account,
        b_vault_lp,
        b_vault,
        b_token_vault,
        b_vault_lp_mint,
        vault_program,
        token_program,
        amount,
    )?;
    
    msg!("✅ SUCESSO: Depósito na pool completado usando WSOL from remaining_accounts");
    Ok(())
}

fn process_deposit_to_pool<'info>(
    user: &AccountInfo<'info>,
    user_source_token: &AccountInfo<'info>,
    b_vault_lp: &AccountInfo<'info>,
    b_vault: &UncheckedAccount<'info>,
    b_token_vault: &AccountInfo<'info>,
    b_vault_lp_mint: &AccountInfo<'info>,
    vault_program: &UncheckedAccount<'info>,
    token_program: &Program<'info, Token>,
    amount: u64,
) -> Result<()> {
    let deposit_accounts = [
        b_vault.to_account_info(),
        b_token_vault.clone(),
        b_vault_lp_mint.clone(),
        user_source_token.clone(),
        b_vault_lp.clone(),
        user.clone(),
        token_program.to_account_info(),
    ];

    let mut deposit_data = Vec::with_capacity(24);
    deposit_data.extend_from_slice(&[242, 35, 198, 137, 82, 225, 242, 182]); // Deposit sighash
    deposit_data.extend_from_slice(&amount.to_le_bytes());
    deposit_data.extend_from_slice(&0u64.to_le_bytes()); // minimum_lp_token_amount = 0

    solana_program::program::invoke(
        &solana_program::instruction::Instruction {
            program_id: vault_program.key(),
            accounts: deposit_accounts.iter().enumerate().map(|(i, a)| {
                if i == 5 {
                    solana_program::instruction::AccountMeta::new_readonly(a.key(), true)
                } else if i < 5 {
                    solana_program::instruction::AccountMeta::new(a.key(), false)
                } else {
                    solana_program::instruction::AccountMeta::new_readonly(a.key(), false)
                }
            }).collect::<Vec<solana_program::instruction::AccountMeta>>(),
            data: deposit_data,
        },
        &deposit_accounts,
    ).map_err(|_| error!(ErrorCode::DepositToPoolFailed))?;
    
    Ok(())
}

fn process_reserve_sol<'info>(
    from: &AccountInfo<'info>,
    to: &AccountInfo<'info>,
    amount: u64,
) -> Result<()> {
    let ix = solana_program::system_instruction::transfer(
        &from.key(),
        &to.key(),
        amount
    );
    
    solana_program::program::invoke(
        &ix,
        &[from.clone(), to.clone()],
    ).map_err(|_| error!(ErrorCode::SolReserveFailed))?;
    
    Ok(())
}

fn process_pay_referrer<'info>(
    from: &AccountInfo<'info>,
    to: &AccountInfo<'info>,
    amount: u64,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    verify_wallet_is_system_account(to)?;
    
    let ix = solana_program::system_instruction::transfer(
        &from.key(),
        &to.key(),
        amount
    );
    
    let mut accounts = Vec::with_capacity(2);
    accounts.push(from.clone());
    accounts.push(to.clone());
    
    solana_program::program::invoke_signed(
        &ix,
        &accounts,
        signer_seeds,
    ).map_err(|_| error!(ErrorCode::ReferrerPaymentFailed))?;
    
    Ok(())
}

pub fn process_mint_tokens<'info>(
    token_mint: &AccountInfo<'info>,
    program_token_vault: &AccountInfo<'info>,
    token_mint_authority: &AccountInfo<'info>,
    token_program: &Program<'info, Token>,
    amount: u64,
    mint_authority_seeds: &[&[&[u8]]],
) -> Result<()> {
    let mint_instruction = spl_token::instruction::mint_to(
        &token_program.key(),
        &token_mint.key(),
        &program_token_vault.key(),
        &token_mint_authority.key(),
        &[],
        amount
    ).map_err(|_| error!(ErrorCode::TokenMintFailed))?;
    
    let mut mint_accounts = Vec::with_capacity(4);
    mint_accounts.push(token_mint.clone());
    mint_accounts.push(program_token_vault.clone());
    mint_accounts.push(token_mint_authority.clone());
    mint_accounts.push(token_program.to_account_info());
    
    solana_program::program::invoke_signed(
        &mint_instruction,
        &mint_accounts,
        mint_authority_seeds,
    ).map_err(|_| error!(ErrorCode::TokenMintFailed))?;
    
    Ok(())
}

pub fn process_transfer_tokens<'info>(
    program_token_vault: &AccountInfo<'info>,
    user_token_account: &AccountInfo<'info>,
    vault_authority: &AccountInfo<'info>,
    token_program: &Program<'info, Token>,
    amount: u64,
    authority_seeds: &[&[&[u8]]],
) -> Result<()> {
    if user_token_account.owner != &spl_token::id() {
        return Err(error!(ErrorCode::TokenAccountInvalid));
    }
    
    let transfer_instruction = spl_token::instruction::transfer(
        &token_program.key(),
        &program_token_vault.key(),
        &user_token_account.key(),
        &vault_authority.key(),
        &[],
        amount
    ).map_err(|_| error!(ErrorCode::TokenTransferFailed))?;
    
    let mut transfer_accounts = Vec::with_capacity(4);
    transfer_accounts.push(program_token_vault.clone());
    transfer_accounts.push(user_token_account.clone());
    transfer_accounts.push(vault_authority.clone());
    transfer_accounts.push(token_program.to_account_info());
    
    solana_program::program::invoke_signed(
        &transfer_instruction,
        &transfer_accounts,
        authority_seeds,
    ).map_err(|_| error!(ErrorCode::TokenTransferFailed))?;
    
    Ok(())
}

fn process_referrer_chain<'info>(
   user_key: &Pubkey,
   referrer: &mut Account<'_, UserAccount>,
   next_chain_id: u32,
) -> Result<(bool, Pubkey)> {
   let slot_idx = referrer.chain.filled_slots as usize;
   if slot_idx >= 3 {
       return Ok((false, referrer.key())); 
   }

   referrer.chain.slots[slot_idx] = Some(*user_key);

   emit!(SlotFilled {
       slot_idx: slot_idx as u8,
       chain_id: referrer.chain.id,
       user: *user_key,
       owner: referrer.key(),
   });

   referrer.chain.filled_slots += 1;

   if referrer.chain.filled_slots == 3 {
       referrer.chain.id = next_chain_id;
       referrer.chain.slots = [None, None, None];
       referrer.chain.filled_slots = 0;

       return Ok((true, referrer.key()));
   }

   Ok((false, referrer.key()))
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = owner,
        space = 8 + ProgramState::SIZE
    )]
    pub state: Account<'info, ProgramState>,
    #[account(mut)]
    pub owner: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(deposit_amount: u64)]
pub struct RegisterWithoutReferrerDeposit<'info> {
    #[account(mut)]
    pub state: Account<'info, ProgramState>,

    #[account(mut)]
    pub owner: Signer<'info>,
    
    #[account(mut)]
    pub user_wallet: Signer<'info>,
    
    #[account(
        init,
        payer = user_wallet,
        space = 8 + UserAccount::SIZE,
        seeds = [b"user_account", user_wallet.key().as_ref()],
        bump
    )]
    pub user: Account<'info, UserAccount>,

    /// CHECK: User token account for Wrapped SOL, verified in the instruction code
    #[account(mut)]
    pub user_source_token: UncheckedAccount<'info>,
    
    /// CHECK: This is the fixed WSOL mint address
    pub wsol_mint: AccountInfo<'info>,

    /// CHECK: Pool account (PDA)
    #[account(mut)]
    pub pool: UncheckedAccount<'info>,

    /// CHECK: Vault account for token B (SOL)
    #[account(mut)]
    pub b_vault: UncheckedAccount<'info>,

    /// CHECK: Token vault account for token B (SOL)
    #[account(mut)]
    pub b_token_vault: UncheckedAccount<'info>,

    /// CHECK: LP token mint for vault B
    #[account(mut)]
    pub b_vault_lp_mint: UncheckedAccount<'info>,

    /// CHECK: LP token account for vault B
    #[account(mut)]
    pub b_vault_lp: UncheckedAccount<'info>,

    /// CHECK: Vault program
    pub vault_program: UncheckedAccount<'info>,

    /// CHECK: Token mint to create the UplineEntry structure
    pub token_mint: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

// 🎯 ESTRUTURA PRINCIPAL - WSOL via remaining_accounts (SEM user_wsol_account)
#[derive(Accounts)]
#[instruction(deposit_amount: u64)]
pub struct RegisterWithSolDeposit<'info> {
    #[account(mut)]
    pub state: Account<'info, ProgramState>,

    #[account(mut)]
    pub user_wallet: Signer<'info>,

    #[account(mut)]
    pub referrer: Account<'info, UserAccount>,
    
    #[account(mut)]
    pub referrer_wallet: SystemAccount<'info>,

    #[account(
        init,
        payer = user_wallet,
        space = 8 + UserAccount::SIZE,
        seeds = [b"user_account", user_wallet.key().as_ref()],
        bump
    )]
    pub user: Account<'info, UserAccount>,
    
    /// CHECK: This is the fixed WSOL mint address - kept for reference
    pub wsol_mint: AccountInfo<'info>,

    /// CHECK: Pool account (PDA)
    #[account(mut)]
    pub pool: UncheckedAccount<'info>,

    /// CHECK: Vault account for token B (SOL)
    #[account(mut)]
    pub b_vault: UncheckedAccount<'info>,

    /// CHECK: Token vault account for token B (SOL)
    #[account(mut)]
    pub b_token_vault: UncheckedAccount<'info>,

    /// CHECK: LP token mint for vault B
    #[account(mut)]
    pub b_vault_lp_mint: UncheckedAccount<'info>,

    /// CHECK: LP token account for vault B
    #[account(mut)]
    pub b_vault_lp: UncheckedAccount<'info>,

    /// CHECK: Vault program
    pub vault_program: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [b"program_sol_vault"],
        bump
    )]
    pub program_sol_vault: SystemAccount<'info>,
    
    /// CHECK: Token mint for minting new tokens
    #[account(mut)]
    pub token_mint: UncheckedAccount<'info>,
    
    /// CHECK: Program token vault to store reserved tokens
    #[account(mut)]
    pub program_token_vault: UncheckedAccount<'info>,
    
    /// CHECK: Referrer's ATA to receive tokens
    #[account(mut)]
    pub referrer_token_account: UncheckedAccount<'info>,
    
    /// CHECK: Mint authority PDA
    #[account(
        seeds = [b"token_mint_authority"],
        bump
    )]
    pub token_mint_authority: UncheckedAccount<'info>,
    
    /// CHECK: Token vault authority
    #[account(
        seeds = [b"token_vault_authority"],
        bump
    )]
    pub vault_authority: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

#[program]
pub mod referral_system {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        if ctx.accounts.owner.key() != admin_addresses::AUTHORIZED_INITIALIZER {
            return Err(error!(ErrorCode::NotAuthorized));
        }

        let state = &mut ctx.accounts.state;
        state.owner = ctx.accounts.owner.key();
        state.multisig_treasury = admin_addresses::MULTISIG_TREASURY;
        state.next_upline_id = 1;
        state.next_chain_id = 1;
        state.last_mint_amount = 0;
        
        Ok(())
    }
    
    pub fn register_without_referrer(ctx: Context<RegisterWithoutReferrerDeposit>, deposit_amount: u64) -> Result<()> {
        if ctx.accounts.owner.key() != ctx.accounts.state.multisig_treasury {
            return Err(error!(ErrorCode::NotAuthorized));
        }
       
        verify_all_fixed_addresses(
            &ctx.accounts.pool.key(),
            &ctx.accounts.b_vault_lp.key(),
            &ctx.accounts.token_mint.key(),
            &verified_addresses::WSOL_MINT,
        )?;

        let state = &mut ctx.accounts.state;
        let upline_id = state.next_upline_id;
        let chain_id = state.next_chain_id;

        state.next_upline_id += 1;
        state.next_chain_id += 1;

        let user = &mut ctx.accounts.user;

        user.is_registered = true;
        user.referrer = None;
        user.owner_wallet = ctx.accounts.user_wallet.key();
        user.upline = ReferralUpline {
            id: upline_id,
            depth: 1,
            upline: vec![],
        };
        user.chain = ReferralChain {
            id: chain_id,
            slots: [None, None, None],
            filled_slots: 0,
        };
        
        user.reserved_sol = 0;
        user.reserved_tokens = 0;

        let sync_native_ix = spl_token::instruction::sync_native(
            &token::ID,
            &ctx.accounts.user_source_token.key(),
        )?;
        
        let sync_accounts = [ctx.accounts.user_source_token.to_account_info()];
        
        solana_program::program::invoke(
            &sync_native_ix,
            &sync_accounts,
        ).map_err(|_| error!(ErrorCode::WrapSolFailed))?;

        process_deposit_to_pool(
            &ctx.accounts.user_wallet.to_account_info(),
            &ctx.accounts.user_source_token.to_account_info(),
            &ctx.accounts.b_vault_lp.to_account_info(),
            &ctx.accounts.b_vault,
            &ctx.accounts.b_token_vault.to_account_info(),
            &ctx.accounts.b_vault_lp_mint.to_account_info(),
            &ctx.accounts.vault_program,
            &ctx.accounts.token_program,
            deposit_amount
        )?;

        Ok(())
    }

    // 🚀 FUNÇÃO PRINCIPAL - WSOL via remaining_accounts
    pub fn register_with_sol_deposit<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, RegisterWithSolDeposit<'info>>, 
        deposit_amount: u64
    ) -> Result<()> {
        if !ctx.accounts.referrer.is_registered {
            return Err(error!(ErrorCode::ReferrerNotRegistered));
        }

        // Verificar se temos contas mínimas necessárias nos remaining_accounts
        if ctx.remaining_accounts.len() < VAULT_A_ACCOUNTS_COUNT + CHAINLINK_ACCOUNTS_COUNT {
            return Err(error!(ErrorCode::MissingVaultAAccounts));
        }

        // Extrair contas Vault A (posições 0, 1, 2)
        let a_vault_lp = &ctx.remaining_accounts[0];
        let a_vault_lp_mint = &ctx.remaining_accounts[1];
        let a_token_vault = &ctx.remaining_accounts[2];

        verify_vault_a_addresses(
            &a_vault_lp.key(),
            &a_vault_lp_mint.key(),
            &a_token_vault.key()
        )?;

        // Extrair contas Chainlink (posições 3, 4)
        let chainlink_feed = &ctx.remaining_accounts[3];
        let chainlink_program = &ctx.remaining_accounts[4];

        verify_all_fixed_addresses(
            &ctx.accounts.pool.key(),
            &ctx.accounts.b_vault_lp.key(),
            &ctx.accounts.token_mint.key(),
            &ctx.accounts.wsol_mint.key(),
        )?;

        verify_chainlink_addresses(
            &chainlink_program.key(),
            &chainlink_feed.key(),
        )?;

        let minimum_deposit = calculate_minimum_sol_deposit(
            chainlink_feed,
            chainlink_program,
        )?;

        if deposit_amount < minimum_deposit {
            msg!("Deposit amount: {}, minimum required: {}", deposit_amount, minimum_deposit);
            return Err(error!(ErrorCode::InsufficientDeposit));
        }

        verify_ata_strict(
            &ctx.accounts.referrer_token_account.to_account_info(),
            &ctx.accounts.referrer_wallet.key(),
            &ctx.accounts.token_mint.key()
        )?;
        
        let referrer_entry = UplineEntry {
            pda: ctx.accounts.referrer.key(),
            wallet: ctx.accounts.referrer_wallet.key(),
        };
        
        let mut new_upline = Vec::new();
        
        if ctx.accounts.referrer.upline.upline.len() >= MAX_UPLINE_DEPTH {
            new_upline.try_reserve(MAX_UPLINE_DEPTH).ok();
            let start_idx = ctx.accounts.referrer.upline.upline.len() - (MAX_UPLINE_DEPTH - 1);
            new_upline.extend_from_slice(&ctx.accounts.referrer.upline.upline[start_idx..]);
        } else {
            new_upline.try_reserve(ctx.accounts.referrer.upline.upline.len() + 1).ok();
            new_upline.extend_from_slice(&ctx.accounts.referrer.upline.upline);
        }
        
        new_upline.push(referrer_entry);
        new_upline.shrink_to_fit();

        let state = &mut ctx.accounts.state;
        let upline_id = state.next_upline_id;
        let chain_id = state.next_chain_id;

        state.next_upline_id += 1;
        state.next_chain_id += 1;

        let user = &mut ctx.accounts.user;

        user.is_registered = true;
        user.referrer = Some(ctx.accounts.referrer.key());
        user.owner_wallet = ctx.accounts.user_wallet.key();
        user.upline = ReferralUpline {
            id: upline_id,
            depth: ctx.accounts.referrer.upline.depth + 1,
            upline: new_upline,
        };
        user.chain = ReferralChain {
            id: chain_id,
            slots: [None, None, None],
            filled_slots: 0,
        };
        
        user.reserved_sol = 0;
        user.reserved_tokens = 0;

        // ===== 🎯 LÓGICA DE SLOTS OTIMIZADA COM WSOL VIA REMAINING_ACCOUNTS =====
        let slot_idx = ctx.accounts.referrer.chain.filled_slots as usize;

        // SLOT 0 (SLOT 1 nos comentários): Usar WSOL da posição 5 nos remaining_accounts
        if slot_idx == 0 {
            msg!("🎯 Processing SLOT 0 (SLOT 1): Using WSOL from remaining_accounts position {}", WSOL_ACCOUNT_POSITION);
            
            // Verificar se temos WSOL account na posição correta
            if ctx.remaining_accounts.len() <= WSOL_ACCOUNT_POSITION {
                msg!("❌ WSOL account missing at position {}", WSOL_ACCOUNT_POSITION);
                return Err(error!(ErrorCode::MissingWsolAccount));
            }
            
            let wsol_account = &ctx.remaining_accounts[WSOL_ACCOUNT_POSITION];
            msg!("📋 WSOL account found at position {}: {}", WSOL_ACCOUNT_POSITION, wsol_account.key());
            
            process_deposit_with_wsol_from_remaining(
                &ctx.accounts.user_wallet,
                wsol_account,
                &ctx.accounts.wsol_mint,
                &ctx.accounts.b_vault_lp.to_account_info(),
                &ctx.accounts.b_vault,
                &ctx.accounts.b_token_vault.to_account_info(),
                &ctx.accounts.b_vault_lp_mint.to_account_info(),
                &ctx.accounts.vault_program,
                &ctx.accounts.token_program,
                deposit_amount,
            )?;
        } 
        // SLOT 1 (SLOT 2 nos comentários): Reservar SOL e mintar tokens (SEM WSOL)
        else if slot_idx == 1 {
            msg!("Processing SLOT 1 (SLOT 2): Reserving SOL and minting tokens (no WSOL needed)");
            
            process_reserve_sol(
                &ctx.accounts.user_wallet.to_account_info(),
                &ctx.accounts.program_sol_vault.to_account_info(),
                deposit_amount
            )?;
            
            ctx.accounts.referrer.reserved_sol = deposit_amount;
            
            let token_amount = get_donut_tokens_amount(
                a_vault_lp,
                &ctx.accounts.b_vault_lp.to_account_info(),
                a_vault_lp_mint,
                &ctx.accounts.b_vault_lp_mint.to_account_info(),
                a_token_vault,
                &ctx.accounts.b_token_vault.to_account_info(),
                deposit_amount
            )?;
            
            let adjusted_token_amount = check_mint_limit(state, token_amount)?;
            force_memory_cleanup();
            
            process_mint_tokens(
                &ctx.accounts.token_mint.to_account_info(),
                &ctx.accounts.program_token_vault.to_account_info(),
                &ctx.accounts.token_mint_authority.to_account_info(),
                &ctx.accounts.token_program,
                adjusted_token_amount,
                &[&[
                    b"token_mint_authority".as_ref(),
                    &[ctx.bumps.token_mint_authority]
                ]],
            )?;
    
            force_memory_cleanup();
            ctx.accounts.referrer.reserved_tokens = adjusted_token_amount;
        }
        // SLOT 2 (SLOT 3 nos comentários): Pagar referrer e processar recursividade (SEM WSOL inicial)
        else if slot_idx == 2 {
            msg!("Processing SLOT 2 (SLOT 3): Paying referrer and starting recursion");
            
            if ctx.accounts.referrer.reserved_sol > 0 {
                verify_wallet_is_system_account(&ctx.accounts.referrer_wallet.to_account_info())?;
                
                process_pay_referrer(
                    &ctx.accounts.program_sol_vault.to_account_info(),
                    &ctx.accounts.referrer_wallet.to_account_info(),
                    ctx.accounts.referrer.reserved_sol,
                    &[&[
                        b"program_sol_vault".as_ref(),
                        &[ctx.bumps.program_sol_vault]
                    ]],
                )?;
                
                ctx.accounts.referrer.reserved_sol = 0;
            }
            
            verify_token_account(
                &ctx.accounts.referrer_token_account.to_account_info(),
                &ctx.accounts.referrer_wallet.key(),
                &ctx.accounts.token_mint.key()
            )?;
            
            if ctx.accounts.referrer.reserved_tokens > 0 {
                process_transfer_tokens(
                    &ctx.accounts.program_token_vault.to_account_info(),
                    &ctx.accounts.referrer_token_account.to_account_info(),
                    &ctx.accounts.vault_authority.to_account_info(),
                    &ctx.accounts.token_program,
                    ctx.accounts.referrer.reserved_tokens,
                    &[&[
                        b"token_vault_authority".as_ref(),
                        &[ctx.bumps.vault_authority]
                    ]],
                )?;
                
                force_memory_cleanup();
                ctx.accounts.referrer.reserved_tokens = 0;
            }
        }
        
        let (chain_completed, upline_pubkey) = process_referrer_chain(
            &ctx.accounts.user_wallet.key(),
            &mut ctx.accounts.referrer,
            state.next_chain_id,
        )?;
    
        force_memory_cleanup();
        
        if chain_completed {
            state.next_chain_id += 1;
        }

        // ===== 🚀 RECURSIVIDADE COM WSOL VIA REMAINING_ACCOUNTS =====
        if chain_completed && slot_idx == 2 {
            msg!("🚀 Starting RECURSION processing with WSOL via remaining_accounts");
            
            let mut current_user_pubkey = upline_pubkey;
            let mut current_deposit = deposit_amount;
    
            // As uplines começam após WSOL account (posição 6 em diante)
            let upline_start_idx = WSOL_ACCOUNT_POSITION + 1;
    
            if ctx.remaining_accounts.len() > upline_start_idx && current_deposit > 0 {
                let upline_accounts = &ctx.remaining_accounts[upline_start_idx..];
                
                if upline_accounts.len() % 3 != 0 {
                    return Err(error!(ErrorCode::MissingUplineAccount));
                }
                
                let trio_count = upline_accounts.len() / 3;
                const BATCH_SIZE: usize = 1; 
                let batch_count = (trio_count + BATCH_SIZE - 1) / BATCH_SIZE;
                
                for batch_idx in 0..batch_count {
                    let start_trio = batch_idx * BATCH_SIZE;
                    let end_trio = std::cmp::min(start_trio + BATCH_SIZE, trio_count);
                    
                    for trio_index in start_trio..end_trio {
                        if trio_index >= MAX_UPLINE_DEPTH || current_deposit == 0 {
                            break;
                        }
    
                        let base_idx = trio_index * 3;
                        let upline_info = &upline_accounts[base_idx];
                        let upline_wallet = &upline_accounts[base_idx + 1];
                        let upline_token = &upline_accounts[base_idx + 2];
                        
                        if upline_wallet.owner != &solana_program::system_program::ID {
                            return Err(error!(ErrorCode::PaymentWalletInvalid));
                        }
                        
                        if !upline_info.owner.eq(&crate::ID) {
                            return Err(error!(ErrorCode::InvalidSlotOwner));
                        }
    
                        let mut upline_account_data;
                        {
                            let data = upline_info.try_borrow_data()?;
                            if data.len() <= 8 {
                                return Err(ProgramError::InvalidAccountData.into());
                            }
    
                            let mut account_slice = &data[8..];
                            upline_account_data = UserAccount::deserialize(&mut account_slice)?;
    
                            if !upline_account_data.is_registered {
                                return Err(error!(ErrorCode::SlotNotRegistered));
                            }
                        }
    
                        force_memory_cleanup();

                        let upline_slot_idx = upline_account_data.chain.filled_slots as usize;
                        let upline_key = *upline_info.key;
                        
                        if upline_slot_idx == 2 {
                            if upline_token.owner != &spl_token::id() {
                                return Err(error!(ErrorCode::TokenAccountInvalid));
                            }
                            
                            verify_token_account(
                                upline_token,
                                &upline_wallet.key(),
                                &ctx.accounts.token_mint.key()
                            )?;
                        }
                        
                        upline_account_data.chain.slots[upline_slot_idx] = Some(current_user_pubkey);
                        
                        emit!(SlotFilled {
                            slot_idx: upline_slot_idx as u8,
                            chain_id: upline_account_data.chain.id,
                            user: current_user_pubkey,
                            owner: upline_key,
                        });
                        
                        upline_account_data.chain.filled_slots += 1;
                        
                        // ===== 🎯 LÓGICA FINANCEIRA NA RECURSIVIDADE COM WSOL OTIMIZADA =====
                        if upline_slot_idx == 0 {
                            msg!("🔄 RECURSION SLOT 0 (SLOT 1): Need WSOL for pool deposit - {} lamports", current_deposit);
                            
                            // Para slot 0 na recursividade, precisamos de WSOL
                            // Verificar se temos WSOL account disponível
                            if ctx.remaining_accounts.len() > WSOL_ACCOUNT_POSITION {
                                let wsol_account = &ctx.remaining_accounts[WSOL_ACCOUNT_POSITION];
                                
                                msg!("📋 Using WSOL account from position {} for recursion: {}", WSOL_ACCOUNT_POSITION, wsol_account.key());
                                
                                // Verificar e usar WSOL para depósito na pool
                                verify_wsol_account(wsol_account, &ctx.accounts.user_wallet.key(), &ctx.accounts.wsol_mint.key())?;
                                
                                process_deposit_to_pool(
                                    &ctx.accounts.user_wallet.to_account_info(),
                                    wsol_account,
                                    &ctx.accounts.b_vault_lp.to_account_info(),
                                    &ctx.accounts.b_vault,
                                    &ctx.accounts.b_token_vault.to_account_info(),
                                    &ctx.accounts.b_vault_lp_mint.to_account_info(),
                                    &ctx.accounts.vault_program,
                                    &ctx.accounts.token_program,
                                    current_deposit,
                                )?;
                                
                                msg!("✅ RECURSION: Pool deposit completed using WSOL");
                            } else {
                                msg!("⚠️ WSOL account not available for recursion, skipping pool deposit");
                            }
                            
                            current_deposit = 0; // Para a recursividade após usar no slot 0
                        } 
                        else if upline_slot_idx == 1 {
                            msg!("🔄 RECURSION SLOT 1 (SLOT 2): Reserving SOL and minting tokens (no WSOL needed)");
                            
                            // SLOT 1 na recursividade: Reservar SOL (SEM WSOL)
                            process_reserve_sol(
                                &ctx.accounts.user_wallet.to_account_info(),
                                &ctx.accounts.program_sol_vault.to_account_info(),
                                current_deposit
                            )?;
                            
                            upline_account_data.reserved_sol = current_deposit;
                            
                            let token_amount = get_donut_tokens_amount(
                                a_vault_lp,
                                &ctx.accounts.b_vault_lp.to_account_info(),
                                a_vault_lp_mint,
                                &ctx.accounts.b_vault_lp_mint.to_account_info(),
                                a_token_vault,
                                &ctx.accounts.b_token_vault.to_account_info(),
                                current_deposit
                            )?;
                            
                            let adjusted_token_amount = check_mint_limit(state, token_amount)?;
                            force_memory_cleanup();
                            
                            process_mint_tokens(
                                &ctx.accounts.token_mint.to_account_info(),
                                &ctx.accounts.program_token_vault.to_account_info(),
                                &ctx.accounts.token_mint_authority.to_account_info(),
                                &ctx.accounts.token_program,
                                adjusted_token_amount,
                                &[&[
                                    b"token_mint_authority".as_ref(),
                                    &[ctx.bumps.token_mint_authority]
                                ]],
                            )?;

                            force_memory_cleanup();
                            
                            upline_account_data.reserved_tokens = adjusted_token_amount;
                            current_deposit = 0; // Para a recursividade após usar no slot 1
                        }
                        else if upline_slot_idx == 2 {
                            msg!("🔄 RECURSION SLOT 2 (SLOT 3): Paying upline (no WSOL needed)");
                            
                            // SLOT 2 na recursividade: Pagar upline (SEM WSOL)
                            if upline_account_data.reserved_sol > 0 {
                                let reserved_sol = upline_account_data.reserved_sol;
                                
                                if upline_wallet.owner != &solana_program::system_program::ID {
                                    return Err(error!(ErrorCode::PaymentWalletInvalid));
                                }
                                
                                let ix = solana_program::system_instruction::transfer(
                                    &ctx.accounts.program_sol_vault.key(),
                                    &upline_wallet.key(),
                                    reserved_sol
                                );
                                
                                let mut accounts = Vec::with_capacity(2);
                                accounts.push(ctx.accounts.program_sol_vault.to_account_info());
                                accounts.push(upline_wallet.clone());
                                
                                solana_program::program::invoke_signed(
                                    &ix,
                                    &accounts,
                                    &[&[
                                        b"program_sol_vault".as_ref(),
                                        &[ctx.bumps.program_sol_vault]
                                    ]],
                                ).map_err(|_| error!(ErrorCode::ReferrerPaymentFailed))?;
                                
                                upline_account_data.reserved_sol = 0;
                            }
                            
                            if upline_account_data.reserved_tokens > 0 {
                                let reserved_tokens = upline_account_data.reserved_tokens;
                                
                                if upline_token.owner != &spl_token::id() {
                                    return Err(error!(ErrorCode::TokenAccountInvalid));
                                }
                                
                                process_transfer_tokens(
                                    &ctx.accounts.program_token_vault.to_account_info(),
                                    upline_token,
                                    &ctx.accounts.vault_authority.to_account_info(),
                                    &ctx.accounts.token_program,
                                    reserved_tokens,
                                    &[&[
                                        b"token_vault_authority".as_ref(),
                                        &[ctx.bumps.vault_authority]
                                    ]],
                                )?;
                                
                                force_memory_cleanup();
                                upline_account_data.reserved_tokens = 0;
                            }
                            // Continua recursividade (não break aqui)
                        }
                        
                        let chain_completed = upline_account_data.chain.filled_slots == 3;
                        
                        if chain_completed {
                            let next_chain_id_value = state.next_chain_id;
                            state.next_chain_id += 1;
                            
                            upline_account_data.chain.id = next_chain_id_value;
                            upline_account_data.chain.slots = [None, None, None];
                            upline_account_data.chain.filled_slots = 0;
                            
                            current_user_pubkey = upline_key;
                        }
                        
                        {
                            let mut data = upline_info.try_borrow_mut_data()?;
                            let mut write_data = &mut data[8..];
                            upline_account_data.serialize(&mut write_data)?;
                        }

                        force_memory_cleanup();
                        
                        if !chain_completed {
                            break;
                        }
                        
                        if trio_index >= MAX_UPLINE_DEPTH - 1 {
                            break;
                        }
                    }
                    
                    if current_deposit == 0 {
                        break;
                    }
                }

                // ===== 🎯 TRATAMENTO DE DEPÓSITO RESTANTE COM WSOL OTIMIZADA =====
                if current_deposit > 0 {
                    msg!("💰 Processing REMAINING DEPOSIT: {} lamports - needs WSOL for pool", current_deposit);
                    
                    // Verificar se temos WSOL account disponível para depósito restante
                    if ctx.remaining_accounts.len() > WSOL_ACCOUNT_POSITION {
                        let wsol_account = &ctx.remaining_accounts[WSOL_ACCOUNT_POSITION];
                        
                        msg!("📋 Using WSOL account from position {} for remaining deposit: {}", WSOL_ACCOUNT_POSITION, wsol_account.key());
                        
                        // Verificar e usar WSOL para depósito restante na pool
                        verify_wsol_account(wsol_account, &ctx.accounts.user_wallet.key(), &ctx.accounts.wsol_mint.key())?;
                        
                        process_deposit_to_pool(
                            &ctx.accounts.user_wallet.to_account_info(),
                            wsol_account,
                            &ctx.accounts.b_vault_lp.to_account_info(),
                            &ctx.accounts.b_vault,
                            &ctx.accounts.b_token_vault.to_account_info(),
                            &ctx.accounts.b_vault_lp_mint.to_account_info(),
                            &ctx.accounts.vault_program,
                            &ctx.accounts.token_program,
                            current_deposit,
                        )?;
                        
                        msg!("✅ REMAINING DEPOSIT: Pool deposit completed using WSOL");
                    } else {
                        msg!("⚠️ WSOL account not available for remaining deposit, skipping");
                    }
                }
            }
        }

        msg!("🎉 Registration completed successfully with WSOL via remaining_accounts optimization!");
        Ok(())
    }
}