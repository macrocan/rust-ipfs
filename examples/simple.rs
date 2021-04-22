use ipfs::{Ipfs, IpfsOptions, TestTypes, UninitializedIpfs};
use libp2p_rs::runtime::task;

fn main() {
    task::block_on(async {
        tracing_subscriber::fmt()
            //.with_max_level(tracing::Level::DEBUG)
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .init();

        // Initialize the repo and start a daemon
        let mut opts = IpfsOptions::inmemory_with_generated_keys();

        opts.listening_addrs
            .push("/ip4/0.0.0.0/tcp/8084".parse().unwrap());
        opts.bootstrap.push((
            "QmUd6zHcbkbcs7SMxwLs48qZVX3vpcM8errYS7xEczwRMA"
                .parse()
                .unwrap(),
            "/ip4/138.201.67.219/tcp/4001".parse().unwrap()));
        opts.bootstrap.push((
            "12D3KooWDsgzyxLH2fTrFeqxJrFxBz4uMEvw4gbR6yeAChSW6ELe"
                .parse()
                .unwrap(),
            "/ip4/127.0.0.1/tcp/4001".parse().unwrap(),
        ));

        let ipfs: Ipfs<TestTypes> = UninitializedIpfs::new(opts).start().await.unwrap();

        let addrs = ipfs.identity().await.unwrap().1;
        println!("I am listening on {:?}", addrs);

        ipfs.run_cli().await;
    });
}
