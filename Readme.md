Fun Pump Launchpad Smart Contract

A secure and feature-rich Solana smart contract for token launches, vesting, and liquidity management.

## Features

### Token Vesting System
- Custom vesting schedules for token distribution
- Time-locked token vesting periods
- Market cap milestone requirements
- Start and end time validation
- Secure token transfer mechanisms

### Token Vault System
- Secure token storage infrastructure
- Time-based locking mechanisms
- Owner-only access control
- Real-time amount tracking
- Automated unlock schedules

### Price Curve Management
- Dynamic market price determination
- Customizable pricing curves (linear/exponential)
- Buy/sell operation handling
- Supply and reserve tracking
- Liquidity pool management

## Security Features

- Owner verification for critical operations
- Time-based validation checks
- Amount validation
- Market cap requirement checks
- Protected token transfers
- Built-in slippage protection
- Comprehensive error handling
- Event logging for all operations

## Core Functions

### Project Owner Operations
```rust
initialize_vesting()  // Set up vesting schedule
initialize_vault()    // Create secure token storage
lock_tokens()        // Lock tokens in vault
Trading Operations
rustCopybuy_tokens()     // Purchase tokens
sell_tokens()    // Sell tokens
Vesting Controls
rustCopylock_tokens_for_vesting()   // Lock tokens in vesting contract
unlock_vested_tokens()      // Release tokens after conditions met
Safety Mechanisms

Minimum/maximum vesting periods
Market cap thresholds
Owner authorization checks
Balance validations
Protected transfer operations
Comprehensive event logging

Anti-Rug Pull Measures

Locked Liquidity
Vesting Schedules
Market Cap Requirements
Time Locks
Owner Verification

Event Tracking
The contract tracks multiple events including:

Vesting initialization
Token locks/unlocks
Trading activities
System initialization
State changes

Error Handling
Comprehensive error handling for:

Authorization failures
Timing violations
Balance issues
Market cap requirements
Calculation errors

Technical Implementation
Vesting Structure
rustCopypub struct Vesting {
    pub owner: Pubkey,
    pub token_mint: Pubkey,
    pub amount: u64,
    pub start_time: i64,
    pub end_time: i64,
    pub target_market_cap: u64,
    pub is_locked: bool,
    pub bump: u8,
}
Vault Structure
rustCopypub struct Vault {
    pub owner: Pubkey,
    pub bump: u8,
    pub locked_amount: u64,
    pub locked_until: i64,
}
Usage

Deploy the smart contract
Initialize vault and vesting schedules
Configure pricing curves
Set up token distribution parameters
Enable trading functions

Best Practices

Always verify transaction signatures
Monitor market cap requirements
Review vesting schedules
Check token balances
Verify owner permissions

License
[Add your license information here]
Contributing
[Add contribution guidelines here]
Support
[Add support information here]




FunPump Smart Contract ## Overview

This FunPump.Ai Solana Anchor program provides mechanisms to:

1. **Initialize a Vault** (PDA) that can hold locked tokens.  
2. **Lock Tokens** into the Vault for a specific duration.  
3. **Unlock Tokens** from the Vault after the lock period.  
4. **Initialize a Vesting** account (another PDA) for a specific amount and schedule.  
5. **Lock Tokens for Vesting** (transfer tokens into a vesting PDA account).  
6. **Unlock Vested Tokens** from the vesting account once both a time condition *and* a market‑cap condition are met.

In essence, this program helps project owners or token issuers implement:

- **Time locks:** For example, locking liquidity or tokens so they cannot be withdrawn prematurely.  
- **Vesting schedules:** Gradual token release or “cliff” release for team members, investors, or other stakeholders, typically with conditions such as time or external triggers.  
- **Optional target conditions:** The code includes an example of requiring a “target_market_cap” before tokens can be unlocked, showing how to incorporate real‑world data or market conditions.  

### Bonding Curves (High‑Level)

While the code as‑is does **not** directly include a bonding‑curve mechanism, you can integrate such a curve for token price discovery or dynamic supply issuance. A “bonding curve” typically means token price or issuance changes depending on how many tokens are purchased (or minted). You can imagine:

- **When users buy tokens:** They call some function that calculates the price using the bonding curve formula (e.g., a polynomial or exponential function).  
- **The tokens they buy** may then be **locked** in a vault or subject to a vesting schedule to prevent immediate dumping.  
- **As the “curve” matures**, prices might go up automatically, or a portion of tokens remains locked until certain conditions (like a target market cap) are met.

Hence, this code can be the **foundation** for the “locking” part of your token launch. You’d add the bonding‑curve logic in a separate instruction or off‑chain aggregator that calculates how many tokens a buyer gets for a certain payment, then calls `lock_tokens` or `initialize_vesting` to hold them under defined rules.

---

## Part 1: Vault Mechanism (Locking & Unlocking)

### 1. Initialize Vault

- **Instruction**: `initialize_vault`  
- **Accounts**:
  - `vault`: A PDA (Program‑Derived Address) that will store the locked tokens.  
  - `payer`: The signer paying the Solana rent and fees to initialize the account.  
  - `system_program`: The standard system program needed to create accounts.  

**Process**:
1. A new `Vault` account is created via PDA seeds (`[b"vault", payer.key().as_ref()]`).  
2. The vault is assigned an `owner` (the `payer` in this example) and a `bump` (used for PDA derivation).  
3. The vault starts with `locked_amount = 0` and `locked_until = 0`.

**When to Use**:  
- Any time a user or project wants to create a new locked vault for their tokens. For example, you might want separate vault PDAs for each unique user, or each user might just have one vault.

### 2. Lock Tokens

- **Instruction**: `lock_tokens`  
- **Accounts**:
  - `vault`: The previously created vault account (PDA) with `has_one = owner`.  
  - `user_token_account`: The SPL Token Account of the user who’s locking tokens.  
  - `vault_token_account`: The SPL Token Account that actually holds the locked tokens on behalf of the vault’s PDA.  
  - `authority`: The signer who owns the `user_token_account` (the user).  
  - `owner`: Must match `vault.owner` (ensures only the correct vault is used).  
  - `token_program`: SPL token program.  

**Process**:
1. Checks that the `amount` is above `MINIMUM_AMOUNT` and that `lock_duration` is within `[MINIMUM_VESTING_PERIOD, MAXIMUM_VESTING_PERIOD]`.  
2. Transfers tokens from `user_token_account` to `vault_token_account`.  
3. Sets `vault.locked_amount = amount` and calculates `vault.locked_until = current_time + lock_duration`.  
4. Emits an event showing the details of the lock.

**When to Use**:  
- Any scenario requiring tokens to be locked for a certain time: e.g., liquidity lock for a DEX listing, team token lock, or sale participants who must hold tokens until a cliff date.

### 3. Unlock Tokens

- **Instruction**: `unlock_tokens`  
- **Accounts**:
  - `vault`: The PDA storing the token lock info.  
  - `user_token_account`: Where the tokens will be sent back upon unlock.  
  - `vault_token_account`: The SPL Token Account controlled by the vault (PDA).  
  - `authority`: The signer requesting unlock.  
  - `owner`: Must match `vault.owner`.  
  - `token_program`: SPL token program.  

**Process**:
1. Checks that the current time `>= vault.locked_until` (otherwise tokens are still locked).  
2. Checks that `vault.locked_amount > 0`.  
3. Performs a CPI transfer **with the vault’s PDA as the authority** to transfer `vault_token_account` → `user_token_account`.  
4. Sets `vault.locked_amount = 0` to reflect the tokens are no longer locked.

**When to Use**:  
- When the lock period has elapsed, or if you have an alternative mechanism that calls this after the time or other conditions are satisfied (like KYC checks, governance approvals, etc.).

---

## Part 2: Vesting Mechanism (Time + Market Cap)

### 4. Initialize Vesting

- **Instruction**: `initialize_vesting`  
- **Accounts**:
  - `owner`: The user creating the vesting schedule.  
  - `token_mint`: Which SPL Token is being vested.  
  - `vesting`: The new vesting PDA account.  
  - `system_program`, `rent`: For account creation.  

**Parameters**:
- `amount`: Total tokens that will be locked/vested.  
- `start_time`: When vesting begins (in Unix timestamp). Must be in the future.  
- `end_time`: When vesting ends, must be strictly greater than `start_time`.  
- `target_market_cap`: Extra condition for unlocking (e.g., you want to ensure the project’s market cap is above a certain threshold).

**Process**:
1. Validates times: `start_time` must be > current time, and `end_time` > `start_time`. The difference must be within `[MINIMUM_VESTING_PERIOD, MAXIMUM_VESTING_PERIOD]`.  
2. Creates the `Vesting` PDA (`[b"vesting", token_mint.key(), owner.key()]`).  
3. Stores the `amount`, `start_time`, `end_time`, `target_market_cap`, sets `is_locked = true`.  

**When to Use**:  
- If you want to schedule a release of tokens that can *only* be unlocked after a specific date (i.e., `end_time`) and an additional condition, in this case a `target_market_cap`.

### 5. Lock Tokens for Vesting

- **Instruction**: `lock_tokens_for_vesting`  
- **Accounts**:
  - `owner`: The user moving tokens into the vesting account.  
  - `vesting`: The vesting PDA (must have `is_locked == true`).  
  - `owner_token_account`: The user’s SPL Token Account from which tokens are transferred.  
  - `vesting_token_account`: The token account belonging to the vesting PDA that will hold the locked tokens.  
  - `token_program`: SPL token program.  

**Process**:
1. Ensures the `amount` sent in matches the `vesting.amount`.  
2. Transfers from `owner_token_account` → `vesting_token_account` using the user’s signature (`owner`).  
3. Emits an event announcing the tokens are locked into vesting.

**When to Use**:  
- After you initialize the vesting schedule, you actually lock the tokens by calling this. Typically, it’s done in the same user flow: create vesting account → lock tokens.

### 6. Unlock Vested Tokens

- **Instruction**: `unlock_vested_tokens`  
- **Accounts**:
  - `owner`: The user who owns the vesting.  
  - `vesting`: The vesting PDA.  
  - `vesting_token_account`: SPL Token Account under the vesting PDA’s control.  
  - `owner_token_account`: Where the tokens will go once they’re unlocked.  
  - `token_program`: SPL token program.  

**Parameters**:
- `current_market_cap`: A number you provide (from an oracle or other source) to compare with the `target_market_cap`.

**Process**:
1. Checks that `is_locked == true`.  
2. Checks that the current time `>= end_time` (vesting period is over).  
3. Checks `current_market_cap >= vesting.target_market_cap`.  
4. Transfers the tokens out of the vesting PDA’s token account → the user’s token account, using the PDA seeds to sign.  
5. Sets `is_locked = false`.  

**When to Use**:  
- Once the time requirement (end_time) **and** market‑cap requirement are fulfilled, the user can reclaim their tokens. This is often used for strategic locks (e.g., tokens for early investors, employees, or founders).

---

## Bonding Curves and Token Launches

Although this code **does not** directly implement bonding curves, here is how you might weave them in:

1. **Bonding Curve Sale**:  
   - A separate instruction or off‑chain script calculates how many tokens a user gets based on their purchase amount and a mathematical curve (e.g., \( price = a \times supply^2 \) or another formula).  

2. **Locking Mechanism**:  
   - Right after purchase, the tokens can be **automatically locked** in a vault. This ensures that new buyers do not dump tokens immediately. You could pass in a parameter like `lock_duration` to define how long each user’s tokens remain locked.  

3. **Vesting Mechanism**:  
   - Alternatively, you might have a “vesting approach” for users who buy at earlier stages of the bonding curve. Early buyers’ tokens might vest linearly over time, or remain fully locked until a certain market cap is reached.  

4. **Target Condition**:  
   - The code already demonstrates how you can incorporate external triggers like `target_market_cap`. For bonding curves, you might track the total minted supply or on‑chain price feed, then use that as a condition to allow partial or full unlock.  

Hence, the “bonding curve” part usually calculates **how many tokens** a user receives. The code here ensures **when** (and under what conditions) they can withdraw them.

---

## Conclusion

In summary, this Anchor program:

- **Manages a Vault** for straightforward time‑based token locking.  
- **Manages a Vesting schedule** for tokens that have a start time, end time, and additional condition (a target market cap).  
- **Uses PDAs** to securely handle locked tokens, so only the program (with the correct seeds) can move tokens out.  
- **Can integrate with bonding curves** by adding a price‑calculation step and piping purchased tokens into either a vault or a vesting schedule.

By following the instructions detailed above, you can enable time‑locked tokens, vested tokens with various conditions, and even integrate more complex token sale logic (like a bonding curve) as your project requires.
