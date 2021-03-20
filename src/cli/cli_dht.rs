use crate::cli::handler;
use cid::Cid;
use libp2p_rs::runtime::task;
use libp2p_rs::xcli::*;
use std::convert::TryFrom;

pub(crate) fn cli_dht_commands<'a>() -> Command<'a> {
    let findprov_dht_cmd = Command::new_with_alias("findprov", "fp")
        .about("Find providers of a Cid")
        .usage("findprov <cid>")
        .action(cli_dht_findprov);
    let provide_dht_cmd = Command::new_with_alias("provide", "pr")
        .about("Provide a Cid to DHT network")
        .usage("provide <cid>")
        .action(cli_dht_provide);

    Command::new_with_alias("dht", "d")
        .about("Interact with DHT")
        .usage("ipfs dht")
        .subcommand(findprov_dht_cmd)
        .subcommand(provide_dht_cmd)
}

fn cli_dht_findprov(app: &App, args: &[&str]) -> XcliResult {
    if args.is_empty() {
        return Err(XcliError::MismatchArgument(1, args.len()));
    }

    let ipfs = handler(app);
    let cid = Cid::try_from(args[0]).map_err(|e| XcliError::BadArgument(e.to_string()))?;

    task::block_on(async {
        let r = ipfs.get_providers(cid).await;
        println!("{:?}", r);
    });

    Ok(CmdExeCode::Ok)
}

fn cli_dht_provide(app: &App, args: &[&str]) -> XcliResult {
    if args.is_empty() {
        return Err(XcliError::MismatchArgument(1, args.len()));
    }

    let ipfs = handler(app);
    let cid = Cid::try_from(args[0]).map_err(|e| XcliError::BadArgument(e.to_string()))?;

    task::block_on(async {
        let r = ipfs.provide(cid).await;
        println!("{:?}", r);
    });

    Ok(CmdExeCode::Ok)
}
