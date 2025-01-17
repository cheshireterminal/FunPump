import dotenv from "dotenv";
import fs from "fs";
import { ComputeBudgetProgram, Connection, Keypair, LAMPORTS_PER_SOL, PublicKey, Transaction } from "@solana/web3.js";
import { DEFAULT_DECIMALS, PumpFunSDK } from "../../src";
import NodeWallet from "@coral-xyz/anchor/dist/cjs/nodewallet";
import { AnchorProvider } from "@coral-xyz/anchor";
import {
  getOrCreateKeypair,
  getSPLBalance,
  printSOLBalance,
  printSPLBalance,
} from "../util";
import { Chain, signAndSendWait, signSendWait, TokenAddress, toUniversal, wormhole } from "@wormhole-foundation/sdk";
import evm from "../../wormhole-sdk-ts/sdk/src/evm.js";
import solana from "../../wormhole-sdk-ts/sdk/src/solana.js";
import { JsonRpcProvider } from '@ethersproject/providers'
import { Wallet, Contract, BigNumber } from 'ethers'
import { createAttestTokenInstruction } from "../../src/attest-token";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { setupCLMM, WETH } from '../clmm'
import { SolanaAddress } from "../../wormhole-sdk-ts/platforms/solana/src/address.js";
import { getSolanaSignAndSendSigner } from "../../wormhole-sdk-ts/platforms/solana/src/signer.js";
import { getEvmSignerForKey } from "../../wormhole-sdk-ts/platforms/evm/src/signer.js";

const KEYS_FOLDER = __dirname + "~";
const SLIPPAGE_BASIS_POINTS = 500n;

// WETH ABI for wrapping/unwrapping and approvals
const WETH_ABI = [
  'function deposit() payable',
  'function withdraw(uint256)',
  'function balanceOf(address) view returns (uint256)',
  'function approve(address, uint256) returns (bool)'
]

// Uniswap V3 Factory ABI
const FACTORY_ABI = [
  'function createPool(address tokenA, address tokenB, uint24 fee) external returns (address pool)',
  'function getPool(address tokenA, address tokenB, uint24 fee) external view returns (address pool)'
]

// Uniswap V3 Pool ABI
const POOL_ABI = [
  'function initialize(uint160 sqrtPriceX96) external',
  'function mint(address recipient, int24 tickLower, int24 tickUpper, uint128 amount, bytes calldata data) external returns (uint256 amount0, uint256 amount1)'
]

const FACTORY_ADDRESS = '0x33128a8fC17869897dcE68Ed026d694621f6FDfD' // Base Mainnet

const main = async () => {
  dotenv.config();

  if (!process.env.HELIUS_RPC_URL) {
    console.error("Please set HELIUS_RPC_URL in .env file");
    console.error(
      "Example: HELIUS_RPC_URL=https://mainnet.helius-rpc.com/?api-key=<your api key>"
    );
    console.error("Get one at: https://www.helius.dev");
    return;
  }

  let connection = new Connection(process.env.HELIUS_RPC_URL || "");

  let wallet = new NodeWallet(new Keypair()); //note this is not used
  const provider = new AnchorProvider(connection, wallet, {
    commitment: "finalized",
  });

  const testAccount = getOrCreateKeypair(KEYS_FOLDER, "new");
  const mint = getOrCreateKeypair(KEYS_FOLDER, "mint");

  await printSOLBalance(
    connection,
    testAccount.publicKey,
    "Test Account keypair"
  );

  let sdk = new PumpFunSDK(provider);

  let globalAccount = await sdk.getGlobalAccount();
  console.log(globalAccount);

  let currentSolBalance = await connection.getBalance(testAccount.publicKey);
  if (currentSolBalance == 0) {
    console.log(
      "Please send some SOL to the test-account:",
      testAccount.publicKey.toBase58()
    );
    return;
  }

  console.log(await sdk.getGlobalAccount());

  //Check if mint already exists
  let boundingCurveAccount = await sdk.getBondingCurveAccount(mint.publicKey);
  if (!boundingCurveAccount) {
    let tokenMetadata = {
      name: "multichain-test",
      symbol: "MCT",
      description: "multichain-test: This is a test token",
      file: await fs.openAsBlob("example/basic/random.png"),
    };

    let createResults = await sdk.createAndBuy(
      testAccount,
      mint,
      tokenMetadata,
      BigInt(0.0001 * LAMPORTS_PER_SOL),
      SLIPPAGE_BASIS_POINTS,
      {
        unitLimit: 250000,
        unitPrice: 250000,
      },
    );

    if (createResults.success) {
      console.log("Success:", `https://pump.fun/${mint.publicKey.toBase58()}`);
      boundingCurveAccount = await sdk.getBondingCurveAccount(mint.publicKey);
      console.log("Bonding curve after create and buy", boundingCurveAccount);
      printSPLBalance(connection, mint.publicKey, testAccount.publicKey);

      // Attest token to Base chain
      const payer = testAccount;
      const messageKeypair = Keypair.generate();

      // Create attest token instruction
      const instruction = createAttestTokenInstruction(
        connection,
        new PublicKey("wormDTUJ6AWPNvk59vGQbDvGJmqbDTdgWgAqcLBCgUb"), // Token Bridge Program ID
        new PublicKey("worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth"), // Wormhole Program ID
        payer.publicKey,
        mint.publicKey,
        messageKeypair.publicKey,
        1 // Nonce
      );

      // Initialize Wormhole SDK
      const wh = await wormhole("Mainnet", [evm, solana]);
      const solanaChain = wh.getChain("Solana");
      const baseChain = wh.getChain("Base");

      // Create and send transaction
      const transaction = new Transaction().add(ComputeBudgetProgram.setComputeUnitPrice({microLamports:333333})).add(instruction);
      transaction.feePayer = payer.publicKey;
      
      const { blockhash } = await connection.getLatestBlockhash();
      transaction.recentBlockhash = blockhash;

      try {
        const signature = await connection.sendTransaction(transaction, [payer]);
        console.log("Attestation transaction signature:", signature);
        
        const confirmation = await connection.confirmTransaction(signature);
        console.log("Attestation transaction confirmed:", confirmation);

        // Get VAA and submit to Base
        const msgs = await solanaChain.parseTransaction(signature);
        const vaa = await wh.getVaa(msgs[0], "TokenBridge:AttestMeta", 60_000);
        
        if (!vaa) throw new Error("VAA not found after timeout");
        
        const tb = await baseChain.getTokenBridge();
        
        const provider = new JsonRpcProvider("https://mainnet.base.org");
        const baseWallet = await getEvmSignerForKey(provider, process.env.BASE_PRIVATE_KEY!);

        console.log("Submitting attestation to Base...");
        const attestTxs = await tb.submitAttestation(vaa);

          const txResponse = await signSendWait(baseChain, attestTxs, baseWallet);
        console.log("Cross-chain attestation complete!");
        // Get token address on Base
        const tokenAddress = await tb.getWrappedAsset({
          chain: "Solana" as Chain,
          address: mint.publicKey.toBase58() as TokenAddress<"Solana">
        });
        console.log("Token address on Base:", tokenAddress);

        // Update the PUMP_TOKEN address in the CLMM setup
        const PUMP_TOKEN_ADDRESS = tokenAddress;
        console.log("PUMP token address set for CLMM:", PUMP_TOKEN_ADDRESS);

        // Bridge initial token balance to Base
        const tokenBalance = await connection.getTokenAccountBalance(
          getAssociatedTokenAddressSync(mint.publicKey, testAccount.publicKey)
        );
        const bridgeAmount = BigInt(tokenBalance.value.amount);

        // Create transfer message
        const transferMsg = await solanaChain.getTokenBridge();
        const transferTx = await transferMsg.transfer(
          new SolanaAddress(testAccount.publicKey),
          {
            chain: "Base" as Chain,
            address: toUniversal("Base", baseWallet.address())
          },
          mint.publicKey.toBase58() as TokenAddress<"Solana">,
          bridgeAmount
        );

        // Execute transfer
        const txs = await signSendWait(
          solanaChain,
          transferTx,
          await getSolanaSignAndSendSigner(connection, testAccount)
        );
        console.log("Bridge transaction submitted");
        for (const tx of txs) {
          try {
            // Wait for and process bridge VAA
            const msgs = await solanaChain.parseTransaction(tx.txid);
            const transferVaa = await wh.getVaa(msgs[0], "TokenBridge:Transfer", 60_000);
            if (!transferVaa) throw new Error("VAA not found after timeout");

            console.log("Redeeming bridged tokens on Base...");
            const redeemTxs = await tb.redeem(toUniversal("Base", baseWallet.address()), transferVaa);
            await signSendWait(baseChain, redeemTxs, baseWallet);
            console.log("Token balance successfully bridged to Base!");
          } catch (error) {
            console.error("Error during bridge VAA processing:", error);
          }
        }

        // Now set up the CLMM with our bridged token
        console.log("Setting up CLMM...");

        // Get WETH contract
        const wethContract = new Contract(WETH.address, WETH_ABI, provider);

        // Get our ETH balance
        const ethBalance = await provider.getBalance(baseWallet.address());
        console.log('ETH Balance:', ethBalance.toString());

        // Wrap all but 0.1 ETH (for gas)
        const wrapAmount = ethBalance.sub(BigNumber.from(10).pow(17)); // Leave 0.1 ETH for gas
        console.log('Wrapping ETH amount:', wrapAmount.toString());

        // Wrap ETH
        const wrapTx = await wethContract.deposit({ value: wrapAmount });
        await wrapTx.wait();
        console.log('ETH wrapped to WETH');

        // Get WETH balance
        const wethBalance = await wethContract.balanceOf(baseWallet.address);
        console.log('WETH Balance:', wethBalance.toString());

        // Setup CLMM with our bridged token
        const { pool, positions } = await setupCLMM(tokenAddress.toString());
        console.log('CLMM setup complete');

        // Create the pool on-chain
        const factory = new Contract(FACTORY_ADDRESS, FACTORY_ABI, provider);
        const createPoolTx = await factory.createPool(
          WETH.address,
          tokenAddress.toString(),
          pool.fee
        );
        const receipt = await createPoolTx.wait();
        console.log('Pool creation tx:', receipt.transactionHash);

        // Get the pool address
        const poolAddress = await factory.getPool(
          WETH.address,
          tokenAddress.toString(),
          pool.fee
        );
        console.log('Pool address:', poolAddress);

        // Get pool contract
        const poolContract = new Contract(poolAddress, POOL_ABI, provider);

        // Initialize pool with starting price
        const initTx = await poolContract.initialize(pool.sqrtRatioX96.toString());
        await initTx.wait();
        console.log('Pool initialized');

        // Initialize pool price and add liquidity for each position
        for (const position of positions) {
          // Approve WETH spending
          const approveTx = await wethContract.approve(poolAddress, position.amount0.toString());
          await approveTx.wait();
          console.log('WETH approved for position');

          // Add liquidity
          const mintTx = await poolContract.mint(
            baseWallet.address,
            position.tickLower,
            position.tickUpper,
            position.liquidity.toString(),
            '0x' // No additional data needed
          );
          const mintReceipt = await mintTx.wait();
          console.log('Position created:', mintReceipt.transactionHash);

          console.log('Position details:');
          console.log('- Tick range:', position.tickLower, 'to', position.tickUpper);
          console.log('- WETH amount:', position.amount0.toString());
          console.log('- Token amount:', position.amount1.toString());
          console.log('- Liquidity:', position.liquidity.toString());
        }

        console.log('CLMM deployment complete!');

      } catch (error) {
        console.error("Error during process:", error);
      }
    }
  } else {
    console.log("boundingCurveAccount", boundingCurveAccount);
    console.log("Success:", `https://pump.fun/${mint.publicKey.toBase58()}`);
    printSPLBalance(connection, mint.publicKey, testAccount.publicKey);
  }
};

main();
