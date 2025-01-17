import { Pool, Position, nearestUsableTick, TickMath, FeeAmount } from '@uniswap/v3-sdk'
import { Token } from '@uniswap/sdk-core'
import JSBI from 'jsbi'

// Constants to match PumpDotFun's bonding curve characteristics
const FEE_TIER = FeeAmount.LOW // 0.3% fee tier
const INITIAL_PRICE = 0.04 // Starting price of 0.04 WETH per token ($200/$5000)
const NUM_POSITIONS = 5 // Number of concentrated positions to create
const PRICE_RANGE_MULTIPLIER = 2 // Each position covers 2x the price range of the previous

// Token definitions (using Base mainnet)
export const WETH = new Token(
  8453, // Base chain ID
  '0x4200000000000000000000000000000000000006', // Base WETH
  18,
  'WETH',
  'Wrapped Ether'
)

// Create pool factory function
function createPool(pumpToken: Token): Pool {
  // Initialize pool with initial price ratio
  const sqrtPriceX96 = Math.sqrt(INITIAL_PRICE) * 2 ** 96
  return new Pool(
    WETH,
    pumpToken,
    FEE_TIER,
    sqrtPriceX96.toString(), // Initial price ratio
    '0', // Initial liquidity
    TickMath.getTickAtSqrtRatio(JSBI.BigInt(Math.floor(sqrtPriceX96))) // Current tick
  )
}

// Helper function to calculate liquidity for a price range
function calculateLiquidityForPriceRange(
  pool: Pool,
  lowerPrice: number,
  upperPrice: number,
  amount0: number,
  amount1: number
): Position {
  // Convert prices to ticks
  const lowerTick = nearestUsableTick(
    Math.floor(Math.log(lowerPrice) / Math.log(1.0001)),
    pool.tickSpacing
  )

  const upperTick = nearestUsableTick(
    Math.floor(Math.log(upperPrice) / Math.log(1.0001)),
    pool.tickSpacing
  )

  // Create position with decreasing liquidity as price increases
  return Position.fromAmounts({
    pool,
    tickLower: lowerTick,
    tickUpper: upperTick,
    amount0: amount0.toString(),
    amount1: amount1.toString(),
    useFullPrecision: true
  })
}

// Create increasing price range positions to match bonding curve
function createBondingCurvePositions(pool: Pool) {
  const positions: Position[] = []
  let currentLowerPrice = INITIAL_PRICE

  for (let i = 0; i < NUM_POSITIONS; i++) {
    const upperPrice = currentLowerPrice * PRICE_RANGE_MULTIPLIER

    // Calculate appropriate liquidity based on price range
    // More liquidity is concentrated in lower price ranges to match bonding curve
    const baseAmount = 100 / Math.pow(2, i) // Exponentially decreasing base amounts
    
    const position = calculateLiquidityForPriceRange(
      pool,
      currentLowerPrice,
      upperPrice,
      baseAmount, // WETH amount
      baseAmount / INITIAL_PRICE // Token amount (adjusted for price ratio)
    )

    positions.push(position)
    
    // Set up next range
    currentLowerPrice = upperPrice
  }

  return positions
}

// Setup function that accepts the attested token address
export async function setupCLMM(attestedTokenAddress?: string) {
  // Create token instance for the bridged token
  const pumpToken = attestedTokenAddress 
    ? new Token(8453, attestedTokenAddress, 18, 'PUMP', 'PumpToken')
    : new Token(8453, '0x0000000000000000000000000000000000000000', 18, 'PUMP', 'PumpToken')
  
  // Create pool
  const pool = createPool(pumpToken)
  
  // Create positions with increasing price ranges
  const positions = createBondingCurvePositions(pool)

  return { pool, positions }
}

// This implementation creates a Uniswap V3 CLMM with:
// 1. Multiple concentrated liquidity positions with increasing price ranges
// 2. More liquidity concentrated in lower price ranges to match bonding curve behavior
// 3. 0.3% fee tier matching typical bonding curve fees
// 4. Price ranges that expand geometrically to cover wider ranges at higher prices
// 5. Initial price of 0.04 WETH per token to match $200/$5000 ratio
// 6. Exponentially decreasing liquidity to match the bonding curve's price discovery mechanism
