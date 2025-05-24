// Script para criar conta WSOL e preparar a transação de registro para multisig treasury
// Dividido em duas etapas independentes

const { Connection, Keypair, PublicKey, SystemProgram, Transaction, ComputeBudgetProgram, TransactionInstruction } = require('@solana/web3.js');
const { AnchorProvider, Program, BN, Wallet, utils } = require('@coral-xyz/anchor');
const fs = require('fs');
const path = require('path');
const bs58 = require('bs58');

// Parâmetros de configuração
const PROGRAM_ID = new PublicKey("2wFmCLVQ8pSF2aKu43gLv2vzasUHhtmAA9HffBDXcRfF");
const STATE_ADDRESS = new PublicKey("2UndNrTvi635pfsM5TZQr9KnMMNS29Ry6mtSCjcBFUyc");
const MULTISIG_TREASURY = new PublicKey("Eu22Js2qTu5bCr2WFY2APbvhDqAhUZpkYKmVsfeyqR2N");
const DEPOSIT_AMOUNT = 100_000_000; // 0.1 SOL

// Endereços importantes diretamente definidos
const TOKEN_PROGRAM_ID = new PublicKey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const ASSOCIATED_TOKEN_PROGRAM_ID = new PublicKey("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
const TOKEN_MINT = new PublicKey("3dCXCZd3cbKHT7jQSLzRNJQYu1zEzaD8FHi4MWHLX4DZ");
const WSOL_MINT = new PublicKey("So11111111111111111111111111111111111111112");
const POOL_ADDRESS = new PublicKey("BEuzx33ecm4rtgjtB2bShqGco4zMkdr6ioyzPh6vY9ot");
const B_VAULT = new PublicKey("FERjPVNEa7Udq8CEv68h6tPL46Tq7ieE49HrE2wea3XT");
const B_TOKEN_VAULT = new PublicKey("HZeLxbZ9uHtSpwZC3LBr4Nubd14iHwz7bRSghRZf5VCG");
const B_VAULT_LP_MINT = new PublicKey("BvoAjwEDhpLzs3jtu4H72j96ShKT5rvZE9RP1vgpfSM");
const B_VAULT_LP = new PublicKey("8mNjx5Aww9DX33uFxZwqb7m2vhsavrxyzkME3hE63sT2");
const VAULT_PROGRAM = new PublicKey("24Uqj9JCLxUeoC3hGfh5W3s9FM9uCHDS2SG3LYwBpyTi");
const RENT_SYSVAR = new PublicKey("SysvarRent111111111111111111111111111111111");
const CHAINLINK_PROGRAM = new PublicKey("HEvSKofvBgfaexv23kMabbYqxasxU3mQ4ibBMEmJWHny");
const SOL_USD_FEED = new PublicKey("99B2bTijsU6f1GCT73HmdR7HCFFjGMBcPZY6jZ96ynrR");
const A_VAULT_LP = new PublicKey("BGh2tc4kagmEmVvaogdcAodVDvUxmXWivYL5kxwapm31");
const A_VAULT_LP_MINT = new PublicKey("Bk33KwVZ8hsgr3uSb8GGNJZpAEqH488oYPvoY5W9djVP");
const A_TOKEN_VAULT = new PublicKey("HoASBFustFYysd9aCu6M3G3kve88j22LAyTpvCNp5J65");

// Função principal
async function main() {
  try {
    console.log("🚀 SCRIPT DE REGISTRO DE USUÁRIO BASE EM DUAS ETAPAS 🚀");
    console.log("=======================================================");
    
    // Receber argumentos da linha de comando
    const args = process.argv.slice(2);
    const walletPath = args[0] || '/root/.config/solana/id.json'; // Carteira normal para criar a conta WSOL
    const mode = args[1] || 'both'; // 'prepare', 'register', or 'both'
    
    // Conexão com a rede
    const connection = new Connection('https://weathered-quiet-theorem.solana-devnet.quiknode.pro/198997b67cb51804baeb34ed2257274aa2b2d8c0', 'confirmed');
    console.log("📡 Conectado à Devnet");
    
    // Carregar carteira normal (para criar a conta WSOL)
    console.log(`🔑 Carregando carteira normal de ${walletPath}...`);
    let walletKeypair;
    try {
      const secretKeyString = fs.readFileSync(walletPath, { encoding: 'utf8' });
      walletKeypair = Keypair.fromSecretKey(
        Uint8Array.from(JSON.parse(secretKeyString))
      );
      console.log(`✅ Carteira carregada: ${walletKeypair.publicKey.toString()}`);
    } catch (e) {
      console.error(`❌ Erro ao carregar carteira: ${e.message}`);
      return;
    }
    
    // Verificar saldo da carteira normal
    const balance = await connection.getBalance(walletKeypair.publicKey);
    console.log(`💰 Saldo da carteira normal: ${balance / 1e9} SOL`);
    
    if (balance < 5000000) { // Pelo menos 0.005 SOL para taxas
      console.error("❌ Saldo insuficiente na carteira normal para pagar taxas");
      return;
    }
    
    // Carregar o IDL
    console.log("📝 Carregando IDL...");
    const idlPath = path.resolve('./target/idl/referral_system.json');
    const idl = require(idlPath);
    console.log("✅ IDL carregado");
    
    // Configurar o provider usando a carteira normal
    const normalWallet = new Wallet(walletKeypair);
    const provider = new AnchorProvider(
      connection,
      normalWallet,
      { commitment: 'confirmed' }
    );
    
    // Inicializar o programa
    const program = new Program(idl, PROGRAM_ID, provider);
    console.log("✅ Programa inicializado");
    
    // Derivar PDA da conta do usuário (para multisig)
    const [userPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("user_account"), MULTISIG_TREASURY.toBuffer()],
      PROGRAM_ID
    );
    console.log(`📄 PDA da conta do usuário (multisig): ${userPDA.toString()}`);
    
    // Verificar se a conta já existe
    try {
      const userInfo = await program.account.userAccount.fetch(userPDA);
      if (userInfo.isRegistered) {
        console.log("⚠️ USUÁRIO JÁ ESTÁ REGISTRADO!");
        console.log(`🆔 Upline ID: ${userInfo.upline.id.toString()}`);
        console.log(`🆔 Chain ID: ${userInfo.chain.id.toString()}`);
        console.log(`📊 Slots preenchidos: ${userInfo.chain.filledSlots}/3`);
        return;
      }
    } catch (e) {
      console.log("✅ Usuário ainda não registrado, prosseguindo...");
    }
    
    // Gerar nova keypair para a conta WSOL temporária
    const tokenKeypair = Keypair.generate();
    const tokenAddress = tokenKeypair.publicKey.toString();
    console.log(`🔑 Nova keypair para conta WSOL gerada: ${tokenAddress}`);
    
    // Salvar a keypair em arquivo com timestamp para evitar sobreposição
    const timestamp = Date.now();
    const keypairData = {
      publicKey: tokenAddress,
      secretKey: Array.from(tokenKeypair.secretKey)
    };
    const keypairFilename = `wsol-temp-keypair-${timestamp}.json`;
    fs.writeFileSync(keypairFilename, JSON.stringify(keypairData, null, 2));
    console.log(`💾 Keypair para WSOL salva em '${keypairFilename}'`);
    
    // ==== ETAPA 1: CRIAR CONTA WSOL TEMPORÁRIA (CARTEIRA NORMAL) ====
    if (mode === 'prepare' || mode === 'both') {
      console.log("\n📋 ETAPA 1: CRIAR E FINANCIAR CONTA WSOL TEMPORÁRIA");
      
      // Calcular espaço necessário e aluguel
      const tokenAccountSpace = 165; // Tamanho padrão para uma conta de token SPL
      const rent = await connection.getMinimumBalanceForRentExemption(tokenAccountSpace);
      const totalAmount = rent + DEPOSIT_AMOUNT;
      
      console.log(`💰 Aluguel para conta WSOL: ${rent / 1e9} SOL`);
      console.log(`💰 Depósito para registro: ${DEPOSIT_AMOUNT / 1e9} SOL`);
      console.log(`💰 Total a ser transferido: ${totalAmount / 1e9} SOL`);
      
      // Criar Transaction para setup da conta WSOL
      const createWsolTx = new Transaction();
      
      // Etapa 1: Criar a conta token
      createWsolTx.add(
        SystemProgram.createAccount({
          fromPubkey: walletKeypair.publicKey,
          newAccountPubkey: tokenKeypair.publicKey,
          lamports: totalAmount,
          space: tokenAccountSpace,
          programId: TOKEN_PROGRAM_ID
        })
      );
      
      // Etapa 2: Inicializar a conta como token WSOL
      // IMPORTANTE: Definir a multisig como owner dos tokens!
      createWsolTx.add(
        new TransactionInstruction({
          keys: [
            { pubkey: tokenKeypair.publicKey, isSigner: false, isWritable: true },
            { pubkey: WSOL_MINT, isSigner: false, isWritable: false },
            { pubkey: MULTISIG_TREASURY, isSigner: false, isWritable: false }, // MULTISIG como owner!
            { pubkey: RENT_SYSVAR, isSigner: false, isWritable: false }
          ],
          programId: TOKEN_PROGRAM_ID,
          data: Buffer.from([1, ...MULTISIG_TREASURY.toBuffer()]) // 1 = Initialize, seguido do owner
        })
      );
      
      // Etapa 3: Sincronizar WSOL
      createWsolTx.add(
        new TransactionInstruction({
          keys: [{ pubkey: tokenKeypair.publicKey, isSigner: false, isWritable: true }],
          programId: TOKEN_PROGRAM_ID,
          data: Buffer.from([17]) // SyncNative instruction code
        })
      );
      
      // Configurar a transação
      createWsolTx.feePayer = walletKeypair.publicKey;
      const blockhash = await connection.getLatestBlockhash();
      createWsolTx.recentBlockhash = blockhash.blockhash;
      
      // Assinar a transação
      createWsolTx.sign(walletKeypair, tokenKeypair);
      
      console.log("📤 Enviando transação para criar conta WSOL...");
      const createTxId = await connection.sendRawTransaction(createWsolTx.serialize());
      console.log(`✅ Transação enviada: ${createTxId}`);
      console.log(`🔍 Link para explorador: https://explorer.solana.com/tx/${createTxId}?cluster=devnet`);
      
      // Aguardar confirmação
      await connection.confirmTransaction({
        signature: createTxId,
        blockhash: blockhash.blockhash,
        lastValidBlockHeight: blockhash.lastValidBlockHeight
      });
      console.log("✅ Conta WSOL criada, inicializada e financiada com sucesso!");
      
      // Verificar saldo da conta WSOL
      try {
        const tokenBalance = await connection.getTokenAccountBalance(tokenKeypair.publicKey);
        console.log(`💰 Saldo da conta WSOL: ${tokenBalance.value.uiAmount} SOL`);
      } catch (e) {
        console.log(`⚠️ Não foi possível verificar o saldo WSOL: ${e.message}`);
      }
    }
    
    // ==== ETAPA 2: PREPARAR TRANSAÇÃO DE REGISTRO PARA MULTISIG ====
    if (mode === 'register' || mode === 'both') {
      console.log("\n📋 ETAPA 2: PREPARAR TRANSAÇÃO DE REGISTRO PARA MULTISIG");
      
      // Verificar se a conta WSOL existe
      try {
        const tokenInfo = await connection.getAccountInfo(tokenKeypair.publicKey);
        if (!tokenInfo) {
          console.error("❌ Conta WSOL não encontrada! Execute a etapa 1 primeiro.");
          return;
        }
        console.log("✅ Conta WSOL verificada.");
      } catch (e) {
        console.error(`❌ Erro ao verificar conta WSOL: ${e.message}`);
        return;
      }
      
      // Preparar os remaining accounts para Vault A e Chainlink
      const remainingAccounts = [
        {pubkey: A_VAULT_LP, isWritable: true, isSigner: false},
        {pubkey: A_VAULT_LP_MINT, isWritable: true, isSigner: false},
        {pubkey: A_TOKEN_VAULT, isWritable: true, isSigner: false},
        {pubkey: SOL_USD_FEED, isWritable: false, isSigner: false},
        {pubkey: CHAINLINK_PROGRAM, isWritable: false, isSigner: false},
      ];
      
      // Aumentar compute units
      const modifyComputeUnits = ComputeBudgetProgram.setComputeUnitLimit({
        units: 1_000_000 // 1 milhão de unidades
      });
      
      // Definir prioridade da transação
      const setPriority = ComputeBudgetProgram.setComputeUnitPrice({
        microLamports: 5000
      });
      
      // Criar instrução de registro
      const registerIx = await program.methods
        .registerWithoutReferrer(new BN(DEPOSIT_AMOUNT))
        .accounts({
          state: STATE_ADDRESS,
          owner: MULTISIG_TREASURY,
          userWallet: MULTISIG_TREASURY,
          user: userPDA,
          userSourceToken: tokenKeypair.publicKey,
          wsolMint: WSOL_MINT,
          pool: POOL_ADDRESS,
          bVault: B_VAULT,
          bTokenVault: B_TOKEN_VAULT,
          bVaultLpMint: B_VAULT_LP_MINT,
          bVaultLp: B_VAULT_LP,
          vaultProgram: VAULT_PROGRAM,
          tokenMint: TOKEN_MINT,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
          rent: RENT_SYSVAR
        })
        .remainingAccounts(remainingAccounts)
        .instruction();
      
      // Criar a transação
      const registerTx = new Transaction();
      registerTx.add(modifyComputeUnits);
      registerTx.add(setPriority);
      
      // Adicionar instrução de sincronização (opcional, mas recomendado para garantir)
      const syncNativeIx = new TransactionInstruction({
        keys: [{ pubkey: tokenKeypair.publicKey, isSigner: false, isWritable: true }],
        programId: TOKEN_PROGRAM_ID,
        data: Buffer.from([17]) // SyncNative instruction code
      });
      registerTx.add(syncNativeIx);
      
      // Adicionar instrução de registro
      registerTx.add(registerIx);
      
      // Configurar a transação
      registerTx.feePayer = MULTISIG_TREASURY; // Multisig como fee payer
      const registerBlockhash = await connection.getLatestBlockhash();
      registerTx.recentBlockhash = registerBlockhash.blockhash;
      
      // Serializar transação para base58 (para importação no Squads)
      const serializedTx = registerTx.serialize({
        requireAllSignatures: false,
        verifySignatures: false
      });
      const base58Tx = bs58.encode(serializedTx);
      
      // Salvar a transação em arquivo
      const txFilename = `tx-registro-base-${timestamp}.txt`;
      fs.writeFileSync(txFilename, base58Tx);
      console.log(`✅ Transação de registro salva em '${txFilename}'`);
      
      // Salvar informações completas em formato JSON
      const infoData = {
        transacao: {
          base58: base58Tx,
          descricao: "Transação de registro de usuário base utilizando conta WSOL temporária",
          instrucoesIncluidas: [
            "setComputeUnitLimit", 
            "setComputeUnitPrice", 
            "syncNative", 
            "registerWithoutReferrer"
          ]
        },
        contas: {
          multisigTreasury: MULTISIG_TREASURY.toString(),
          userPDA: userPDA.toString(),
          wsolTemporaria: tokenKeypair.publicKey.toString(),
          programId: PROGRAM_ID.toString(),
          stateAddress: STATE_ADDRESS.toString()
        },
        valores: {
          depositAmount: DEPOSIT_AMOUNT,
          depositAmountSOL: DEPOSIT_AMOUNT / 1e9
        },
        wsolKeypair: keypairData
      };
      
      const infoFilename = `info-registro-base-${timestamp}.json`;
      fs.writeFileSync(infoFilename, JSON.stringify(infoData, null, 2));
      console.log(`✅ Informações detalhadas salvas em '${infoFilename}'`);
      
      // Imprimir instruções para o usuário
      console.log("\n📋 INSTRUÇÕES PARA REGISTRO COM MULTISIG:");
      console.log("1. Acesse o Squads: https://app.squads.so");
      console.log("2. Conecte a carteira associada ao multisig Treasury");
      console.log("3. Crie uma nova transação usando 'Import from base58'");
      console.log(`4. Cole a string base58 do arquivo '${txFilename}'`);
      console.log("5. Colete as assinaturas necessárias e execute a transação");
      
      console.log("\n📋 RESUMO DOS ENDEREÇOS:");
      console.log(`🏦 Multisig Treasury: ${MULTISIG_TREASURY.toString()}`);
      console.log(`📄 PDA da conta do usuário: ${userPDA.toString()}`);
      console.log(`💰 Conta WSOL temporária: ${tokenKeypair.publicKey.toString()}`);
      console.log(`💰 Valor de depósito: ${DEPOSIT_AMOUNT / 1e9} SOL`);
    }
    
    console.log("\n🎉 SCRIPT CONCLUÍDO COM SUCESSO! 🎉");
    
  } catch (error) {
    console.error("\n❌ ERRO DURANTE A EXECUÇÃO DO SCRIPT:");
    console.error(error);
    
    if (error.logs) {
      console.log("\n📋 LOGS DE ERRO DETALHADOS:");
      error.logs.forEach((log, i) => console.log(`${i}: ${log}`));
    }
  }
}

// Execução da função principal
main();