use core::{
    error::Error as SpectreError,
    tip_context::TipContext,
    tip_transition_wallet::TipTransitionWallet,
    utils::{get_tx_explorer_url, try_parse_required_nonzero_spectre_as_sompi_u64},
};
use std::sync::Arc;

use spectre_wallet_core::tx::{Fees, PaymentOutputs};
use spectre_wallet_keys::secret::Secret;
//adding more imports to teloxide!~
use teloxide::{prelude::Requester, Bot, types::{Message, ChatId, UserId}};
use user::user::TipUser;
use workflow_core::prelude::Abortable;

use crate::error::TelegramBotError;

pub async fn command_send(
    bot: Bot,
    _msg: &Message,
    tip_context: Arc<TipContext>,
    tip_sender: &TipUser,
    password: String,
    amount: String,
    recipient_username: String,
    tip_message: Option<String>, //optional tip message!
) -> Result<(), TelegramBotError> {
    let is_opened = tip_context.does_opened_owned_wallet_exists(tip_sender.wallet_identifier());
    let is_initiated = match is_opened {
        true => true,
        false => {
            tip_context
                .local_store()?
                .exists(Some(tip_sender.wallet_identifier()))
                .await?
        }
    };

    /* 
    putting pseudo channel ID for use for send function:
    
    using get_chat_member function (or is it a struct??), we can use a channel ID to find our recipient

    (context/source --> from teloxide reference) vvvvvvvvvv

        pub fn new(chat_id: impl Into<Recipient>, user_id: UserId) -> GetChatMember
                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
                        |
                        --> https://docs.rs/teloxide/latest/teloxide/types/enum.Recipient.html
                            |
                            --> "A unique identifier for the target chat or username 
                                of the target channel (in the format @channelusername)."

    *Creating a placeholder for channel ID below!*
    */ 
    // had copilot help try to define the correct type for channel ID, unsure if this is correct :(
    let channel_id = teloxide::types::Recipient::ChannelUsername("@totally_legit_channel_woohoo!".to_string());

    // FOR USE TIP MESSAGE FEATURE -> need to convert recipient_username to user_id:
        let recipient_chat_id = bot
            .get_chat(recipient_username.clone())
            .await
            .map_err(|_| TelegramBotError::Custom("Error finding user!".to_string()))?;
            
        //convert from ChatId to UserId: copilot helped with converting to correct types! I don't know if/how citing generated responses is neccessary :0
        let user_id = UserId(recipient_chat_id.id.0.try_into().map_err(|_| 
            TelegramBotError::Custom("integer64 value, I am a bot and I expected unsigned64 value beep boop".to_string()))?);

    if !is_initiated {
        tip_sender
            .send_telegram_message(&bot, "Error, wallet not initiated yet")
            .await?;

        return Ok(());
    }

    if !is_opened {
        tip_sender
            .send_telegram_message(&bot, "Error, wallet not opened")
            .await?;

        return Ok(());
    }

    let tip_wallet = match tip_context.get_opened_owned_wallet(tip_sender.wallet_identifier()) {
        Some(w) => w,
        None => {
            tip_sender
                .send_telegram_message(&bot, "Unexpected error: wallet not opened")
                .await?;

            return Ok(());
        }
    };

    let amount_sompi = try_parse_required_nonzero_spectre_as_sompi_u64(Some(amount))?;
    println!("amount sompi {}", amount_sompi);

    let wallet = tip_wallet.wallet();

    // @TODO(izio/tg) since we're unable to confirm the recipient exists, we need to double confirm with end-user (discussion flow)
    let recipient_identifier = format!("tg-{}", recipient_username.to_lowercase());

    // find address of recipient or create a temporary wallet
    let existing_owned_wallet = tip_context
        .owned_wallet_metadata_store
        .find_owned_wallet_metadata_by_owner_identifier(&recipient_identifier)
        .await;

    let recipient_address = match existing_owned_wallet {
        Ok(wallet) => wallet.receive_address,
        Err(SpectreError::OwnedWalletNotFound()) => {
            // find or create a temporary wallet
            let transition_wallet_result = tip_context
                .transition_wallet_metadata_store
                .find_transition_wallet_metadata_by_identifier_couple(
                    tip_sender.wallet_identifier(),
                    &recipient_identifier,
                )
                .await?;

            match transition_wallet_result {
                Some(wallet) => wallet.receive_address,
                None => TipTransitionWallet::create(
                    tip_context.clone(),
                    tip_sender.wallet_identifier(),
                    &recipient_identifier,
                )
                .await?
                .receive_address(),
            }
        }
        Err(e) => {
            tip_sender
                .send_telegram_message(&bot, format!("Error: {:}", e))
                .await?;
            return Ok(());
        }
    };

    let address = recipient_address;

    let outputs = PaymentOutputs::from((address, amount_sompi));
    let abortable = Abortable::default();
    let wallet_secret = Secret::from(password);

    let account = wallet.account()?;

    let (summary, hashes) = match account
        .send(
            outputs.into(),
            Fees::SenderPays(0),
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
        .await
    {
        Ok(result) => result,
        Err(e) => {
            tip_sender
                .send_telegram_message(&bot, format!("Error, transaction failed: {:}", e))
                .await?;

            return Ok(());
        }
    };

    let tx_id = hashes[0].to_string();

    // public mentionning
    tip_sender
        .send_telegram_message(
            &bot,
            format!(
                "Transaction Successful: you sent @{}: {}\n\nTxid: {:?}\nExplorer: {}",
                recipient_identifier,
                summary,
                tx_id,
                get_tx_explorer_url(&tx_id, tip_context.network_id().network_type())
            ),
        )
        .await?;

    // private message sent to user from bot~
    if let Some(message) = tip_message {
        // error (red squigglies) for get_chat_member -> I think i'm just writing it down wrong :0
        let recipient_chat_id = bot
            .get_chat_member(channel_id, user_id)
            .await
            .map_err(|_| TelegramBotError::Custom("Failed to get reciever chat ID".to_string()))?;

        bot.send_message(recipient_chat_id.user.id, message).await?;
    }
    // @TODO(izio/tg): how to deal with private mentionning? Is this even possible?
    //  -> https://stackoverflow.com/a/78459144  ~ Is this what you had mentioned to me?


    Ok(())
}
