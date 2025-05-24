// verificar.js
const { Connection, PublicKey } = require('@solana/web3.js');
const { AnchorProvider, Program, utils } = require('@coral-xyz/anchor');
const fs = require('fs');
const path = require('path');

// Receber parâmetros da linha de comando
const args = process.argv.slice(2);
const walletPath = args[0] || '/root/.config/solana/id.json'; // Caminho padrão se não for fornecido
const stateAddress = args[1]; // Endereço do estado a verificar
const configOutputPath = args[2] || './matriz-config-atual.json';

// Carregar seu IDL compilado
const idl = require('./target/idl/referral_system.json');

// Configurações principais
const PROGRAM_ID = new PublicKey("2wFmCLVQ8pSF2aKu43gLv2vzasUHhtmAA9HffBDXcRfF");
const TOKEN_MINT = new PublicKey("3dCXCZd3cbKHT7jQSLzRNJQYu1zEzaD8FHi4MWHLX4DZ");
const MULTISIG_TREASURY = new PublicKey("Eu22Js2qTu5bCr2WFY2APbvhDqAhUZpkYKmVsfeyqR2N");

// Função para carregar uma carteira a partir de um arquivo
function loadWalletFromFile(filePath) {
  if (!fs.existsSync(filePath)) {
    throw new Error(`Arquivo de carteira não encontrado: ${filePath}`);
  }
  return require('@solana/web3.js').Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(fs.readFileSync(filePath, 'utf-8')))
  );
}

// Função para formatar valores de token com 9 casas decimais
function formatTokenAmount(amount) {
  if (amount === 0) return "0";
  const amountStr = amount.toString().padStart(10, '0');
  const decimalPos = amountStr.length - 9;
  const integerPart = amountStr.substring(0, decimalPos) || '0';
  const decimalPart = amountStr.substring(decimalPos);
  return `${integerPart}.${decimalPart}`;
}

async function main() {
  try {
    console.log("🔍 VERIFICANDO ESTADO ATUAL DO PROGRAMA DE MATRIZ 🔍");
    console.log("===============================================================");
    console.log(`Usando arquivo de carteira: ${walletPath}`);
    
    if (!stateAddress) {
      console.error("❌ Erro: Endereço do estado não fornecido!");
      console.log("Por favor, forneça o endereço do estado como segundo parâmetro:");
      console.log("node verificar.js /caminho/para/carteira.json ENDEREÇO_DO_ESTADO");
      return;
    }
    
    const statePublicKey = new PublicKey(stateAddress);
    console.log(`🔑 Endereço do estado a verificar: ${statePublicKey.toString()}`);
    
    // Conectar à devnet
    const connection = new Connection('https://weathered-quiet-theorem.solana-devnet.quiknode.pro/198997b67cb51804baeb34ed2257274aa2b2d8c0', 'confirmed');
    console.log('Conectando à Devnet');
    
    // Carregar carteira
    let walletKeypair;
    try {
      walletKeypair = loadWalletFromFile(walletPath);
      console.log("👤 Endereço da carteira: " + walletKeypair.publicKey.toString());
    } catch (e) {
      console.error(`❌ Erro ao carregar carteira: ${e.message}`);
      return;
    }
    
    // Verificar saldo da carteira
    const balance = await connection.getBalance(walletKeypair.publicKey);
    console.log(`💰 Saldo: ${balance / 1_000_000_000} SOL`);
    
    // Configurar o provider com a carteira
    const provider = new AnchorProvider(
      connection, 
      { 
        publicKey: walletKeypair.publicKey, 
        signTransaction: async (tx) => {
          tx.partialSign(walletKeypair);
          return tx;
        }, 
        signAllTransactions: async (txs) => {
          return txs.map(tx => {
            tx.partialSign(walletKeypair);
            return tx;
          });
        }
      },
      { commitment: 'confirmed' }
    );
    
    // Inicializar o programa
    const program = new Program(idl, PROGRAM_ID, provider);
    
    // Verificar informações do estado
    try {
      const stateInfo = await program.account.programState.fetch(statePublicKey);
      console.log("\n📊 INFORMAÇÕES DO ESTADO DA MATRIZ:");
      console.log("👑 Owner: " + stateInfo.owner.toString());
      console.log("🏦 Multisig Treasury: " + stateInfo.multisigTreasury.toString());
      console.log("🆔 Próximo ID de upline: " + stateInfo.nextUplineId.toString());
      console.log("🆔 Próximo ID de chain: " + stateInfo.nextChainId.toString());
      
      // Exibir o valor do last_mint_amount, se estiver disponível no estado
      if (stateInfo.lastMintAmount !== undefined) {
        // Convertendo explicitamente para evitar erros de tipo
        const lastMintAmountBigInt = BigInt(stateInfo.lastMintAmount.toString());
        const lastMintAmountNumber = Number(lastMintAmountBigInt);
        
        console.log(`🔒 Limitador de Mintagem (último valor): ${lastMintAmountBigInt.toString()} (${formatTokenAmount(lastMintAmountNumber)} DONUT)`);
        
        // Calcular o limite com conversões explícitas para evitar erro de tipo
        if (lastMintAmountBigInt === 0n) {
          console.log(`🔒 Próximo limite máximo (3x): 0 (0 DONUT)`);
          console.log(`ℹ️ Nota: O primeiro mint não terá limite (valor inicial = 0)`);
        } else {
          const limitBigInt = lastMintAmountBigInt * 3n;
          const limitNumber = Number(limitBigInt);
          console.log(`🔒 Próximo limite máximo (3x): ${limitBigInt.toString()} (${formatTokenAmount(limitNumber)} DONUT)`);
        }
      } else {
        console.log("ℹ️ Campo lastMintAmount não encontrado no estado - verifique se o contrato foi atualizado com este campo");
      }
      
      // Verificar PDAs necessárias para integração
      console.log("\n🔑 PDAS PARA INTEGRAÇÃO:");
      
      // PDA para autoridade de mintagem
      const [tokenMintAuthority, tokenMintAuthorityBump] = PublicKey.findProgramAddressSync(
        [Buffer.from("token_mint_authority")],
        PROGRAM_ID
      );
      console.log("🔑 PDA Mint Authority: " + tokenMintAuthority.toString() + " (Bump: " + tokenMintAuthorityBump + ")");
      
      // PDA para vault de SOL
      const [programSolVault, programSolVaultBump] = PublicKey.findProgramAddressSync(
        [Buffer.from("program_sol_vault")],
        PROGRAM_ID
      );
      console.log("💰 PDA do Vault de SOL: " + programSolVault.toString() + " (Bump: " + programSolVaultBump + ")");
      
      // PDA para autoridade do vault de tokens
      const [vaultAuthority, vaultAuthorityBump] = PublicKey.findProgramAddressSync(
        [Buffer.from("token_vault_authority")],
        PROGRAM_ID
      );
      console.log("🔑 PDA do Vault Authority: " + vaultAuthority.toString() + " (Bump: " + vaultAuthorityBump + ")");
      
      // Calcular ATA do vault de tokens
      const programTokenVault = utils.token.associatedAddress({
        mint: TOKEN_MINT,
        owner: vaultAuthority
      });
      console.log("💰 ATA do Vault de Tokens: " + programTokenVault.toString());
      
      // Verificar se a ATA já existe
      try {
        const ataInfo = await connection.getAccountInfo(programTokenVault);
        if (ataInfo) {
          console.log("✅ ATA do Vault já existe!");
          
          // Verificar saldo da ATA
          try {
            const tokenBalance = await connection.getTokenAccountBalance(programTokenVault);
            console.log(`💎 Saldo de tokens no vault: ${tokenBalance.value.uiAmount} DONUT`);
          } catch (e) {
            console.log(`⚠️ Erro ao verificar saldo de tokens: ${e.message}`);
          }
        } else {
          console.log("⚠️ ATA do Vault ainda não foi criada");
          console.log("💡 Você pode criar a ATA do vault usando o script de inicialização ou um script específico");
        }
      } catch (e) {
        console.log("⚠️ Erro ao verificar ATA do vault: " + e.message);
      }
      
      // Verificar saldo do vault de SOL
      try {
        const solVaultBalance = await connection.getBalance(programSolVault);
        console.log(`💰 Saldo do Vault de SOL: ${solVaultBalance / 1_000_000_000} SOL`);
      } catch (e) {
        console.log("⚠️ Erro ao verificar saldo do vault de SOL: " + e.message);
      }
      
      // Gravar todas as informações importantes em um arquivo de configuração
      const configData = {
        programId: PROGRAM_ID.toString(),
        stateAddress: statePublicKey.toString(),
        tokenMint: TOKEN_MINT.toString(),
        tokenMintAuthority: tokenMintAuthority.toString(),
        tokenMintAuthorityBump,
        programSolVault: programSolVault.toString(),
        programSolVaultBump,
        vaultAuthority: vaultAuthority.toString(),
        vaultAuthorityBump,
        programTokenVault: programTokenVault.toString(),
        ownerWallet: stateInfo.owner.toString(),
        multisigTreasury: stateInfo.multisigTreasury.toString(),
        nextUplineId: stateInfo.nextUplineId.toString(),
        nextChainId: stateInfo.nextChainId.toString(),
        lastMintAmount: stateInfo.lastMintAmount !== undefined ? stateInfo.lastMintAmount.toString() : '0'
      };
      
      // Criar diretório para o arquivo de configuração se não existir
      const configDir = path.dirname(configOutputPath);
      if (!fs.existsSync(configDir)) {
        fs.mkdirSync(configDir, { recursive: true });
      }
      
      fs.writeFileSync(configOutputPath, JSON.stringify(configData, null, 2));
      console.log(`\n💾 Configuração atual salva em ${configOutputPath}`);
      
      console.log("\n⚠️ IMPORTANTE: ENDEREÇOS PARA USO FUTURO!");
      console.log("🔑 ENDEREÇO DO PROGRAMA: " + PROGRAM_ID.toString());
      console.log("🔑 ESTADO DO PROGRAMA: " + statePublicKey.toString());
      console.log("🔑 OWNER DO PROGRAMA: " + stateInfo.owner.toString());
      console.log("🏦 MULTISIG TREASURY: " + stateInfo.multisigTreasury.toString());
      console.log("🔑 PDA MINT AUTHORITY: " + tokenMintAuthority.toString());
      console.log("🔑 PDA SOL VAULT: " + programSolVault.toString());
      console.log("🔑 PDA VAULT AUTHORITY: " + vaultAuthority.toString());
      console.log("🔑 ATA DO VAULT DE TOKENS: " + programTokenVault.toString());
      
      // Adicionar informação sobre o sistema de limitação de mintagem
      if (stateInfo.lastMintAmount !== undefined) {
        console.log("\n🔒 INFORMAÇÕES DO LIMITADOR DE MINTAGEM:");
        console.log(`🔒 Valor inicial: ${stateInfo.lastMintAmount.toString()} (${formatTokenAmount(Number(stateInfo.lastMintAmount))} DONUT)`);
        console.log(`ℹ️ O limitador de mintagem começa com valor zero e será atualizado automaticamente`);
        console.log(`ℹ️ após o primeiro mint. Valores acima de 3x o último mint serão ajustados para`);
        console.log(`ℹ️ usar o valor do último mint, garantindo que o contrato continue funcionando`);
        console.log(`ℹ️ mesmo após períodos de inatividade ou alta volatilidade.`);
      }
      
    } catch (error) {
      console.error("❌ ERRO AO VERIFICAR ESTADO DA MATRIZ:", error);
      console.log("Verifique se o endereço do estado está correto e se a conta existe.");
      
      // Exibir detalhes do erro para diagnóstico
      if (error.logs) {
        console.log("\n📋 LOGS DE ERRO:");
        error.logs.forEach((log, i) => console.log(`${i}: ${log}`));
      }
    }
  } catch (error) {
    console.error("❌ ERRO GERAL DURANTE O PROCESSO:", error);
  } finally {
    process.exit(0);
  }
}

main();


//node verificar.js /root/.config/solana/id.json 2UndNrTvi635pfsM5TZQr9KnMMNS29Ry6mtSCjcBFUyc