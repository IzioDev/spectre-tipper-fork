use teloxide::macros::BotCommands;

pub mod change_password;
pub mod claim;
pub mod close;
pub mod create;
pub mod destroy;
pub mod export;
pub mod open;
pub mod restore;
pub mod send;
pub mod status;
pub mod withdraw;

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
pub enum Command {
    #[command(description = "display this text.")]
    Help,
    #[command(description = "display the node's network ID.")]
    NetworkId,
    #[command(
        description = "<password>, create (initiate) a fresh discord wallet protected by a password of your choice.",
        parse_with = "split"
    )]
    Create { password: String },
    #[command(description = "get the status of your wallet.")]
    Status,
    #[command(description = "<password>, open your wallet with the password you set.")]
    Open { password: String },
    #[command(description = "close your wallet.")]
    Close,
    #[command(description = "destroy your wallet.")]
    Destroy,
    #[command(description = "<password>, export your wallet.")]
    Export { password: String },
    #[command(
        description = "<old_password> <new_password>, change your wallet's password.",
        parse_with = "split"
    )]
    ChangePassword {
        old_password: String,
        new_password: String,
    },
    #[command(
        description = "<mnemonic> <password>, restore your wallet from a backup.",
        parse_with = "split"
    )]
    Restore { mnemonic: String, password: String },
    #[command(
        description = "<telegram_username> <amount> <password> [optional] <message>, tip a user with a specific amount. You may include an optional message~",
        parse_with = "split"
    )]
    Send {
        telegram_username: String,
        amount: String,
        password: String,

        message: Option<String>, // optional tip message!

    },
    #[command(
        description = "<address> <amount> <password>, withdraw funds from your wallet.",
        parse_with = "split"
    )]
    Withdraw {
        address: String,
        amount: String,
        password: String,
    },
    #[command(description = "claim your tip(s).")]
    Claim,
}
