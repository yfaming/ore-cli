mod balance;
mod busses;
mod claim;
mod cu_limits;
#[cfg(feature = "admin")]
mod initialize;
mod mine;
mod register;
mod rewards;
mod send_and_confirm;
mod treasury;
#[cfg(feature = "admin")]
mod update_admin;
#[cfg(feature = "admin")]
mod update_difficulty;
mod utils;

use anyhow::Result;
use clap::{command, Parser, Subcommand};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    hash::Hash,
    signature::{read_keypair_file, Keypair},
};
use std::sync::Arc;
use std::sync::Mutex;

pub struct Miner {
    pub keypair: Keypair,
    pub priority_fee: u64,

    pub rpc_url: String,
    pub rpc_client: RpcClient,
    latest_blockhash: Arc<Mutex<(Hash, u64)>>,
}

#[derive(Parser, Debug)]
#[command(about, version)]
struct Args {
    #[arg(
        long,
        value_name = "NETWORK_URL",
        help = "Network address of your RPC provider",
        global = true
    )]
    rpc: Option<String>,

    #[clap(
        global = true,
        short = 'C',
        long = "config",
        id = "PATH",
        help = "Filepath to config file."
    )]
    pub config_file: Option<String>,

    #[arg(
        long,
        value_name = "KEYPAIR_FILEPATH",
        help = "Filepath to keypair to use",
        global = true
    )]
    keypair: Option<String>,

    #[arg(
        long,
        value_name = "MICROLAMPORTS",
        help = "Number of microlamports to pay as priority fee per transaction",
        default_value = "0",
        global = true
    )]
    priority_fee: u64,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Fetch the Ore balance of an account")]
    Balance(BalanceArgs),

    #[command(about = "Fetch the distributable rewards of the busses")]
    Busses(BussesArgs),

    #[command(about = "Mine Ore using local compute")]
    Mine(MineArgs),

    #[command(about = "Claim available mining rewards")]
    Claim,

    #[command(about = "Fetch your balance of unclaimed mining rewards")]
    Rewards(RewardsArgs),

    #[command(about = "Fetch the treasury account and balance")]
    Treasury,

    #[cfg(feature = "admin")]
    #[command(about = "Initialize the program")]
    Initialize(InitializeArgs),

    #[cfg(feature = "admin")]
    #[command(about = "Update the program admin authority")]
    UpdateAdmin(UpdateAdminArgs),

    #[cfg(feature = "admin")]
    #[command(about = "Update the mining difficulty")]
    UpdateDifficulty(UpdateDifficultyArgs),
}

#[derive(Parser, Debug)]
struct BalanceArgs {
    #[arg(
        // long,
        value_name = "ADDRESS",
        help = "The address of the account to fetch the balance of"
    )]
    pub address: Option<String>,
}

#[derive(Parser, Debug)]
struct BussesArgs {}

#[derive(Parser, Debug)]
struct RewardsArgs {
    #[arg(
        // long,
        value_name = "ADDRESS",
        help = "The address of the account to fetch the rewards balance of"
    )]
    pub address: Option<String>,
}

#[derive(Parser, Debug)]
struct MineArgs {
    #[arg(
        long,
        short,
        value_name = "THREAD_COUNT",
        help = "The number of threads to dedicate to mining",
        default_value = "1"
    )]
    threads: u64,
}

#[cfg(feature = "admin")]
#[derive(Parser, Debug)]
struct InitializeArgs {}

#[cfg(feature = "admin")]
#[derive(Parser, Debug)]
struct UpdateAdminArgs {
    new_admin: String,
}

#[cfg(feature = "admin")]
#[derive(Parser, Debug)]
struct UpdateDifficultyArgs {}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Load the config file from custom path, the default path, or use default config values
    let cli_config = if let Some(config_file) = &args.config_file {
        solana_cli_config::Config::load(config_file).unwrap_or_else(|_| {
            eprintln!("error: Could not find config file `{}`", config_file);
            std::process::exit(1);
        })
    } else if let Some(config_file) = &*solana_cli_config::CONFIG_FILE {
        solana_cli_config::Config::load(config_file).unwrap_or_default()
    } else {
        solana_cli_config::Config::default()
    };

    // Initialize miner.
    let cluster = args.rpc.unwrap_or(cli_config.json_rpc_url);
    let default_keypair = args.keypair.unwrap_or(cli_config.keypair_path);

    let miner = Miner::new(cluster.clone(), args.priority_fee, &default_keypair).await?;
    tokio::spawn(poll_latest_blockhash(
        miner.clone_rpc_client(),
        miner.latest_blockhash.clone(),
    ));

    // Execute user command.
    match args.command {
        Commands::Balance(args) => {
            miner.balance(args.address).await;
        }
        Commands::Busses(_) => {
            miner.busses().await;
        }
        Commands::Rewards(args) => {
            miner.rewards(args.address).await;
        }
        Commands::Treasury => {
            miner.treasury().await;
        }
        Commands::Mine(args) => {
            miner.mine(args.threads).await;
        }
        Commands::Claim => {
            miner.claim().await;
        }
        #[cfg(feature = "admin")]
        Commands::Initialize(_) => {
            miner.initialize().await;
        }
        #[cfg(feature = "admin")]
        Commands::UpdateAdmin(args) => {
            miner.update_admin(args.new_admin).await;
        }
        #[cfg(feature = "admin")]
        Commands::UpdateDifficulty(_) => {
            miner.update_difficulty().await;
        }
    }

    Ok(())
}

impl Miner {
    pub async fn new(rpc_url: String, priority_fee: u64, keypair_filepath: &str) -> Result<Self> {
        let keypair = read_keypair_file(keypair_filepath).map_err(|e| anyhow::anyhow!("{}", e))?;
        let rpc_client =
            RpcClient::new_with_commitment(rpc_url.clone(), CommitmentConfig::confirmed());

        let blockhash = rpc_client
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .await?;

        let latest_blockhash = Arc::new(Mutex::new(blockhash));

        Ok(Self {
            keypair,
            priority_fee,
            rpc_url,
            rpc_client,
            latest_blockhash,
        })
    }

    pub fn clone_rpc_client(&self) -> RpcClient {
        RpcClient::new_with_commitment(self.rpc_url.clone(), CommitmentConfig::confirmed())
    }

    pub fn signer(&self) -> &Keypair {
        &self.keypair
    }

    pub fn get_latest_blockhash(&self) -> (Hash, u64) {
        let lock = self.latest_blockhash.lock().unwrap();
        *lock
    }
}

pub async fn poll_latest_blockhash(
    rpc_client: RpcClient,
    latest_blockhash: Arc<Mutex<(Hash, u64)>>,
) -> ! {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        let blockhash = match rpc_client
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .await
        {
            Ok(blockhash) => blockhash,
            Err(e) => {
                println!("Failed to fetch latest blockhash: {:?}", e);
                continue;
            }
        };

        let mut lock = latest_blockhash.lock().unwrap();
        *lock = blockhash;
    }
}
