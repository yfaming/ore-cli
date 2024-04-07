use crate::utils::get_proof;
use crate::{cu_limits::CU_LIMIT_CLAIM, Miner};
use solana_program::pubkey::Pubkey;
use solana_sdk::{compute_budget::ComputeBudgetInstruction, signature::Signer};

impl Miner {
    pub async fn claim(&self) {
        let proof = get_proof(&self.rpc_client, self.signer().pubkey()).await;
        let amount = proof.claimable_rewards;
        let amountf = (amount as f64) / (10f64.powf(ore::TOKEN_DECIMALS as f64));
        if amountf < 0.0 {
            println!("nothing to claim, exit now.");
            return;
        } else {
            println!("claimable rewards: {:} ORE", amountf);
        }

        let beneficiary = self.initialize_ata().await;
        let cu_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(CU_LIMIT_CLAIM);
        let cu_price_ix = ComputeBudgetInstruction::set_compute_unit_price(self.priority_fee);
        let ix = ore::instruction::claim(self.signer().pubkey(), beneficiary, amount);
        println!("Submitting claim transaction...");
        match self
            .send_and_confirm(&[cu_limit_ix, cu_price_ix, ix], false, false)
            .await
        {
            Ok(sig) => {
                println!("Claimed {:} ORE to account {:}", amountf, beneficiary);
                println!("{:?}", sig);
            }
            Err(err) => {
                println!("Error: {:?}", err);
            }
        }
    }

    async fn initialize_ata(&self) -> Pubkey {
        // Initialize client.
        let signer = self.signer();

        // Build instructions.
        let token_account_pubkey = spl_associated_token_account::get_associated_token_address(
            &signer.pubkey(),
            &ore::MINT_ADDRESS,
        );

        // Check if ata already exists
        if let Ok(Some(_ata)) = self
            .rpc_client
            .get_token_account(&token_account_pubkey)
            .await
        {
            return token_account_pubkey;
        }

        // Sign and send transaction.
        let ix = spl_associated_token_account::instruction::create_associated_token_account(
            &signer.pubkey(),
            &signer.pubkey(),
            &ore::MINT_ADDRESS,
            &spl_token::id(),
        );
        println!("Creating token account {}...", token_account_pubkey);
        match self.send_and_confirm(&[ix], true, false).await {
            Ok(_sig) => println!("Created token account {:?}", token_account_pubkey),
            Err(e) => println!("Transaction failed: {:?}", e),
        }

        // Return token account address
        token_account_pubkey
    }
}
