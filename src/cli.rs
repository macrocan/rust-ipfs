mod cli_block;
mod cli_dag;
mod cli_unixfs;
mod cli_dht;

mod cli_bitswap;
pub(crate) use cli_bitswap::bitswap_cli_commands;

use crate::{Ipfs, TestTypes};
use libp2p_rs::xcli::*;

const IPFS: &str = "ipfs";

pub(crate) fn ipfs_cli_commands<'a>() -> Command<'a> {
    Command::new_with_alias(IPFS, "i")
        .about("IPFS")
        .usage("ipfs")
        .subcommand(cli_unixfs::cli_add_commands())
        .subcommand(cli_unixfs::cli_cat_commands())
        .subcommand(cli_unixfs::cli_get_commands())
        .subcommand(cli_block::cli_block_commands())
        .subcommand(cli_dag::cli_dag_commands())
        .subcommand(cli_dht::cli_dht_commands())
}

pub(crate) fn handler(app: &App) -> Ipfs<TestTypes> {
    let value_any = app.get_handler(IPFS).expect(IPFS);
    let ipfs = value_any
        .downcast_ref::<Ipfs<TestTypes>>()
        .expect("ipfs")
        .clone();
    ipfs
}
