use std::{
    env,
    ffi::{OsStr, OsString},
    path::Path,
    str::FromStr,
    sync::Arc,
};

use core::{
    error::Error as SpectreError,
    tip_context::{self, TipContext},
    tip_owned_wallet::TipOwnedWallet,
    tip_transition_wallet::TipTransitionWallet,
    utils::try_parse_required_nonzero_spectre_as_sompi_u64,
};
use directories::BaseDirs;
use poise::{
    serenity_prelude::{self as serenity, Colour, CreateEmbed},
    CreateReply, Modal,
};
use spectre_addresses::Address;

use spectre_wallet_core::{
    prelude::{Language, Mnemonic},
    rpc::ConnectOptions,
    tx::PaymentOutputs,
    wallet::Wallet,
};
use spectre_wallet_keys::secret::Secret;
use spectre_wrpc_client::{prelude::NetworkId, Resolver, SpectreRpcClient, WrpcEncoding};
use workflow_core::abortable::Abortable;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::ApplicationContext<'a, Arc<TipContext>, Error>;

// TODO: mutualize embed creation (avoid repetition and centralize calls) and reply in general
// TODO: move cmd to dedicated files

#[poise::command(
    slash_command,
    subcommands(
        "create", "open", "close", "restore", "status", "destroy", "send", "debug"
    ),
    category = "wallet"
)]
/// Main command for interracting with the discord wallet
async fn wallet(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

#[poise::command(slash_command, category = "wallet")]
/// create (initiate) a fresh discord wallet with a secret
async fn create(
    ctx: Context<'_>,
    #[min_length = 10]
    #[description = "secret"]
    secret: String,
) -> Result<(), Error> {
    let embed = CreateEmbed::new();

    if secret.len() < 10 {
        let errored_embed = embed
            .clone()
            .title("Error while restoring the wallet")
            .description("Secret must be greater than 10")
            .colour(Colour::DARK_RED);

        ctx.send(CreateReply {
            reply: false,
            embeds: vec![errored_embed],
            ephemeral: Some(true),
            ..Default::default()
        })
        .await?;
    }

    let user = ctx.author().id;
    let wallet_owner_identifier = user.to_string();

    let tip_context = ctx.data();

    let is_opened = tip_context.does_opened_owned_wallet_exists(&wallet_owner_identifier);
    let is_initiated = match is_opened {
        true => true,
        false => {
            tip_context
                .local_store()?
                .exists(Some(&wallet_owner_identifier))
                .await?
        }
    };

    if is_initiated {
        ctx.say(format!("a discord wallet already exists",)).await?;

        return Ok(());
    }

    let (tip_wallet, mnemonic) = TipOwnedWallet::create(
        tip_context.clone(),
        &Secret::from(secret),
        &wallet_owner_identifier,
    )
    .await?;

    ctx.say(format!(
        "{}\n{}",
        mnemonic.phrase(),
        tip_wallet.receive_address()
    ))
    .await?;

    Ok(())
}

#[poise::command(slash_command, category = "wallet")]
/// open the discord wallet using the secret
async fn open(
    ctx: Context<'_>,
    #[min_length = 10]
    #[description = "secret"]
    secret: String,
) -> Result<(), Error> {
    let embed = CreateEmbed::new();

    if secret.len() < 10 {
        let errored_embed = embed
            .clone()
            .title("Error while restoring the wallet")
            .description("Secret must be greater than 10")
            .colour(Colour::DARK_RED);

        ctx.send(CreateReply {
            reply: false,
            embeds: vec![errored_embed],
            ephemeral: Some(true),
            ..Default::default()
        })
        .await?;

        return Ok(());
    }

    let user = ctx.author().id;
    let wallet_owner_identifier = user.to_string();

    let tip_context = ctx.data();

    // already opened
    if let Some(wallet) = tip_context.get_opened_owned_wallet(&wallet_owner_identifier) {
        ctx.say(format!("{}", wallet.receive_address())).await?;

        return Ok(());
    }

    let tip_wallet_result = TipOwnedWallet::open(
        tip_context.clone(),
        &Secret::from(secret),
        &wallet_owner_identifier,
    )
    .await;

    let tip_wallet = match tip_wallet_result {
        Ok(t) => t,
        Err(SpectreError::WalletError(spectre_wallet_core::error::Error::WalletDecrypt(_))) => {
            let errored_embed = embed
                .clone()
                .title("Error while opening the wallet")
                .description("Secret is wrong")
                .colour(Colour::DARK_RED);

            ctx.send(CreateReply {
                reply: false,
                embeds: vec![errored_embed],
                ephemeral: Some(true),
                ..Default::default()
            })
            .await?;

            return Ok(());
        }
        Err(error) => return Err(Error::from(error)),
    };

    ctx.say(format!("{}", tip_wallet.receive_address())).await?;

    Ok(())
}

#[poise::command(slash_command, category = "wallet")]
/// close the opened discord wallet
async fn close(ctx: Context<'_>) -> Result<(), Error> {
    let tip_context = ctx.data;

    let user = ctx.author().id;
    let wallet_owner_identifier = user.to_string();

    let is_opened = tip_context.does_opened_owned_wallet_exists(&wallet_owner_identifier);

    if is_opened {
        let tip_wallet_result = tip_context.remove_opened_owned_wallet(&wallet_owner_identifier);

        if let Some(tip_wallet) = tip_wallet_result {
            tip_wallet.wallet().close().await?;
        }
    }

    ctx.say(format!("wallet closed")).await?;

    Ok(())
}

#[poise::command(slash_command, category = "wallet")]
/// get the status of your discord wallet
async fn status(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author().id;
    let wallet_owner_identifier = user.to_string();

    let tip_context = ctx.data();

    let is_opened = tip_context.does_opened_owned_wallet_exists(&wallet_owner_identifier);
    let is_initiated = match is_opened {
        true => true,
        false => {
            tip_context
                .local_store()?
                .exists(Some(&wallet_owner_identifier))
                .await?
        }
    };

    ctx.say(format!(
        "is opened: {}\nis_initiated{}",
        is_opened, is_initiated
    ))
    .await?;

    Ok(())
}

#[poise::command(slash_command, category = "wallet")]
/// dev cmd
async fn debug(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author().id;
    let wallet_owner_identifier = user.to_string();

    let tip_context = ctx.data();

    let is_opened = tip_context.does_opened_owned_wallet_exists(&wallet_owner_identifier);
    let is_initiated = match is_opened {
        true => true,
        false => {
            tip_context
                .local_store()?
                .exists(Some(&wallet_owner_identifier))
                .await?
        }
    };

    let wallet = Arc::new(Wallet::try_new(Wallet::local_store()?, None, None)?);

    let descriptors = wallet.account_descriptors().await?;

    ctx.say(format!(
        "is opened: {}\nis_initiated{}\n{:?}",
        is_opened, is_initiated, descriptors
    ))
    .await?;

    Ok(())
}

#[derive(Debug, poise::Modal)]
#[name = "Confirm wallet destruction"]
struct DestructionModalConfirmation {
    #[name = "write detroy to confirm"]
    first_input: String,
}

#[poise::command(slash_command, category = "wallet")]
/// destroy your existing (if exists) discord wallet
async fn destroy(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author().id;
    let wallet_owner_identifier = user.to_string();

    let tip_context = ctx.data();

    let is_opened = tip_context.does_opened_owned_wallet_exists(&wallet_owner_identifier);
    let is_initiated = match is_opened {
        true => true,
        false => {
            tip_context
                .local_store()?
                .exists(Some(&wallet_owner_identifier))
                .await?
        }
    };

    if !is_initiated {
        ctx.say(format!(
            "the wallet is not initiated, cannot destroy a non existing thing"
        ))
        .await?;

        return Ok(());
    }

    let result = DestructionModalConfirmation::execute(ctx).await?;

    if let Some(data) = result {
        if data.first_input == "destroy" {
            if is_opened {
                let tip_wallet_result =
                    tip_context.remove_opened_owned_wallet(&wallet_owner_identifier);

                if let Some(tip_wallet) = tip_wallet_result {
                    tip_wallet.wallet().close().await?;
                };
            }

            // TODO: erase the file on file system, current storage implementation disallow this via direct API access

            ctx.say(format!("destroy ok")).await?;

            return Ok(());
        }
    }

    ctx.say(format!("destroy aborted")).await?;

    return Ok(());
}

#[poise::command(slash_command)]
/// restore a wallet from the mnemonic
async fn restore(
    ctx: Context<'_>,
    #[description = "mnemonic"] mnemonic_phrase: String,
    #[min_length = 10]
    #[description = "new secret"]
    secret: String,
) -> Result<(), Error> {
    let embed = CreateEmbed::new();

    if secret.len() < 10 {
        let errored_embed = embed
            .clone()
            .title("Error while restoring the wallet")
            .description("Secret must be greater than 10")
            .colour(Colour::DARK_RED);

        ctx.send(CreateReply {
            reply: false,
            embeds: vec![errored_embed],
            ephemeral: Some(true),
            ..Default::default()
        })
        .await?;
    }

    let errored_embed = embed
        .clone()
        .title("Error while restoring the wallet")
        .description("Mnemonic is not valid")
        .colour(Colour::DARK_RED);

    let reply = CreateReply {
        reply: false,
        embeds: vec![errored_embed],
        ephemeral: Some(true),
        ..Default::default()
    };

    // try cast mnemonic_prase as Mnemonic
    let mnemonic = match Mnemonic::new(mnemonic_phrase, Language::default()) {
        Ok(r) => r,
        Err(_) => {
            ctx.send(reply).await?;
            return Ok(());
        }
    };

    let user = ctx.author().id;
    let wallet_owner_identifier = user.to_string();

    let tip_context = ctx.data();

    let recovered_tip_wallet_result = TipOwnedWallet::restore(
        tip_context.clone(),
        &Secret::from(secret),
        mnemonic,
        &wallet_owner_identifier,
    )
    .await?;

    ctx.say(recovered_tip_wallet_result.receive_address())
        .await?;
    Ok(())
}

#[poise::command(slash_command, category = "wallet")]
/// send to user the given amount
async fn send(
    ctx: Context<'_>,
    #[description = "Send to"] user: serenity::User,
    #[description = "Amount"] amount: String,
    #[description = "Wallet Secret"] secret: String,
) -> Result<(), Error> {
    if user.bot || user.system {
        ctx.say("user is a bot or a system user").await?;
        return Ok(());
    }

    let recipiant_identifier = user.id.to_string();

    let response = format!(
        "{}'s account was created at {}",
        user.name,
        user.created_at()
    );
    ctx.say(response).await?;

    let author = ctx.author().id;
    let wallet_owner_identifier = author.to_string();

    let tip_context = ctx.data();

    let is_opened = tip_context.does_opened_owned_wallet_exists(&wallet_owner_identifier);
    let is_initiated = match is_opened {
        true => true,
        false => {
            tip_context
                .local_store()?
                .exists(Some(&wallet_owner_identifier))
                .await?
        }
    };

    if !is_initiated {
        ctx.say("wallet not initiated yet").await?;
        return Ok(());
    }

    if !is_opened {
        ctx.say("wallet not opened").await?;
        return Ok(());
    }

    let tip_wallet = match tip_context.get_opened_owned_wallet(&wallet_owner_identifier) {
        Some(w) => w,
        None => {
            ctx.say("unexpected error: wallet not opened").await?;
            return Ok(());
        }
    };

    let wallet = tip_wallet.wallet();

    // find address of recipiant or create a temporary wallet
    let existing_owned_wallet = tip_context
        .owned_wallet_metadata_store
        .find_owned_wallet_metadata_by_owner_identifier(&recipiant_identifier)
        .await;

    let recipiant_address = match existing_owned_wallet {
        Ok(wallet) => wallet.receive_address,
        Err(SpectreError::OwnedWalletNotFound()) => {
            // find or create a temporary wallet
            let transition_wallet_result = tip_context
                .transition_wallet_metadata_store
                .find_transition_wallet_metadata_for_identifier_couple(
                    &author.to_string(),
                    &recipiant_identifier,
                )
                .await?;

            match transition_wallet_result {
                Some(wallet) => wallet.receive_address,
                None => TipTransitionWallet::create(
                    tip_context.clone(),
                    &author.to_string(),
                    &recipiant_identifier,
                )
                .await?
                .receive_address(),
            }
        }
        Err(e) => {
            ctx.say(format!("Error: {:}", e)).await?;
            return Ok(());
        }
    };

    let address = recipiant_address;

    let amount_sompi = try_parse_required_nonzero_spectre_as_sompi_u64(Some(amount))?;

    println!("amount sompi {}", amount_sompi);

    let outputs = PaymentOutputs::from((address, amount_sompi));

    let abortable = Abortable::default();

    let wallet_secret = Secret::from(secret);

    let account = wallet.account()?;

    let (summary, hashes) = account
        .send(
            outputs.into(),
            i64::from(0).into(),
            None,
            wallet_secret,
            None,
            &abortable,
            Some(Arc::new(
                move |ptx: &spectre_wallet_core::tx::PendingTransaction| {
                    println!("tx notifier: {:?}", ptx);
                },
            )),
        )
        .await?;

    ctx.say(format!("{summary} {:?}", hashes)).await?;

    Ok(())
}

#[tokio::main]
async fn main() {
    // env
    dotenvy::dotenv().unwrap();

    let discord_token = match env::var("DISCORD_TOKEN") {
        Ok(v) => v,
        Err(_) => panic!("DISCORD_TOKEN environment variable is missing."),
    };

    let spectre_network_str =
        env::var("SPECTRE_NETWORK").expect("SPECTRE_NETWORK environment variable is missing");

    let wallet_data_path_str =
        env::var("WALLET_DATA_PATH").expect("WALLET_DATA_PATH environment variable is missing");

    // RPC
    let forced_spectre_node: Option<String> = match env::var("FORCE_SPECTRE_NODE_ADDRESS") {
        Ok(v) => Some(v),
        Err(_) => None,
    };

    let resolver = match forced_spectre_node.clone() {
        Some(value) => Resolver::new(vec![Arc::new(value)]),
        _ => Resolver::default(),
    };

    let network_id = NetworkId::from_str(&spectre_network_str).unwrap();

    let wrpc_client = Arc::new(
        SpectreRpcClient::new(
            WrpcEncoding::Borsh,
            forced_spectre_node.as_deref(),
            Some(resolver.clone()),
            Some(network_id.clone()),
            None,
        )
        .unwrap(),
    );

    wrpc_client
        .connect(Some(ConnectOptions {
            url: forced_spectre_node.clone(),
            block_async_connect: true,
            ..Default::default()
        }))
        .await
        .unwrap();

    // @TODO(@izio): create the folder if it doesn't exists, on first run it crash otherwise
    let wallet_data_path_buf = BaseDirs::new()
        .unwrap()
        .data_dir()
        .join(wallet_data_path_str.clone());

    let tip_context = TipContext::try_new_arc(
        resolver,
        NetworkId::from_str(&spectre_network_str).unwrap(),
        forced_spectre_node,
        wrpc_client,
        wallet_data_path_buf,
    );

    if let Err(e) = tip_context {
        panic!("{}", format!("Error while building tip context: {}", e));
    }

    // discord
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![wallet()],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(tip_context.unwrap())
            })
        })
        .build();

    let intents = serenity::GatewayIntents::non_privileged();
    let client = serenity::ClientBuilder::new(discord_token, intents)
        .framework(framework)
        .await;
    client.unwrap().start().await.unwrap();
}
