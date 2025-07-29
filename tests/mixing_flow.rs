use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

fn run(cmd: &str, args: &[&str]) -> String {
    println!("> Running: {} {}", cmd, args.join(" "));
    let output = Command::new(cmd)
        .args(args)
        .output()
        .expect("Failed to run command");

    if !output.status.success() {
        panic!(
            "Command `{}` failed: {}",
            cmd,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn get_usdc_balance(addr: &str, rpc: &str, token: &str) -> u128 {
    run("cast", &["balance", addr, "--rpc-url", rpc, "--erc20", token])
        .split_whitespace()
        .next()
        .unwrap()
        .parse::<u128>()
        .unwrap()
}

fn describe_evm_tx(tx_id: &str, rpc: &str) {
    println!("> EVM TX Receipt:");
    let _ = run("cast", &["receipt", tx_id, "--rpc-url", rpc]);
}

#[test]
fn test_usdc_mixing_flow() {
    dotenv::dotenv().ok();

    let sepolia_rpc_url = "https://ethereum-sepolia-rpc.publicnode.com";
    let usdc_address = "0x1c7D4B196Cb0C7B01d743Fbc6116a902379C7238";
    let bridge_address = "0x0b03df1D4B3884b8987254D0C990342B571183AF";
    let miden_usdc = "0x63c9d7af451fda2000fa06ce0bdefd";
    let evm_min_balance = 1000000000000000u128;

    if !std::path::Path::new("miden-client.toml").exists() {
        println!("miden-client.toml not found. Initializing...");
        run("miden-bridge", &["init", "--network", "testnet"]);
    }

    let privatekey = std::env::var("TEST_PRIVATE_KEY").expect("Missing TEST_PRIVATE_KEY");
    let receiver_address = std::env::var("TEST_RECEIVER_ADDRESS").expect("Missing TEST_RECEIVER_ADDRESS");
    let transfer_amount: f64 = std::env::var("TEST_USDC_AMOUNT").expect("Missing TEST_USDC_AMOUNT").parse().unwrap();
    let amount = (transfer_amount * 1_000_000.0) as u128;

    let address = run("cast", &["wallet", "address", "--private-key", &privatekey]);
    println!("Sender: {}", address);

    let balance = run("cast", &["balance", &address, "--rpc-url", sepolia_rpc_url])
        .parse::<u128>().unwrap();
    println!("Sender ETH: {:.4}", balance as f64 / 1e18);
    assert!(balance > evm_min_balance);

    let usdc_balance_before = get_usdc_balance(&address, sepolia_rpc_url, usdc_address);
    println!("Sender USDC: {:.2}", usdc_balance_before as f64 / 1e6);
    assert!(usdc_balance_before >= amount);

    let receiver_before = get_usdc_balance(&receiver_address, sepolia_rpc_url, usdc_address);
    println!("Receiver USDC before: {:.2}", receiver_before as f64 / 1e6);

    println!("> Approval TX");
    let approve_tx = run("cast", &[
        "mktx", "-r", sepolia_rpc_url,
        "--private-key", &privatekey,
        "-f", &address,
        usdc_address, "approve(address,uint256)", bridge_address, &amount.to_string(),
    ]);
    let approve_tx_id = run("cast", &["publish", "--async", "-r", sepolia_rpc_url, &approve_tx]);
    println!("Approval tx id: {}", approve_tx_id);
    describe_evm_tx(&approve_tx_id, sepolia_rpc_url);

    sleep(Duration::from_secs(40));

    println!("> Get recipient...");
    let recipient_response = run("miden-bridge", &[
        "recipient", "--note-type", "crosschain",
        "--dest-chain", "11155111",
        "--dest-address", &receiver_address,
    ]);
    println!("{}", recipient_response);

    let hexes: Vec<_> = recipient_response
        .split_whitespace()
        .filter(|s| s.starts_with("0x"))
        .collect();
    let bridge_note_serial_number = hexes[0];
    let recipient = hexes[1];
    let serial_number = hexes[2];

    println!("> Bridge TX");
    let bridge_tx = run("cast", &[
        "mktx", "-r", sepolia_rpc_url,
        "--private-key", &privatekey,
        "-f", &address,
        bridge_address,
        "bridgeAndCall(address,uint256,uint32,address,address,bytes,bool)",
        usdc_address, &amount.to_string(), "9966",
        "0x0000000000000000000000000000000000000000",
        "0x0000000000000000000000000000000000000000",
        recipient, "false",
    ]);
    let bridge_tx_id = run("cast", &["publish", "--async", "-r", sepolia_rpc_url, &bridge_tx]);
    println!("Bridge tx id: {}", bridge_tx_id);
    describe_evm_tx(&bridge_tx_id, sepolia_rpc_url);

    for i in 1..=5 {
        println!("Waiting relayer attempt {}/5...", i);
        sleep(Duration::from_secs(90));

        let mix = Command::new("miden-bridge")
            .args(&[
                "mix",
                "--serial-number", serial_number,
                "--bridge-serial-number", bridge_note_serial_number,
                "--dest-chain", "11155111",
                "--dest-address", &receiver_address,
                "--asset-amount", &amount.to_string(),
                "--faucet-id", miden_usdc,
            ])
            .output();

        if let Ok(output) = mix {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                println!("MIX output:\n{stdout}");

                sleep(Duration::from_secs(90));
                let receiver_after = get_usdc_balance(&receiver_address, sepolia_rpc_url, usdc_address);
                let delta = receiver_after.saturating_sub(receiver_before);
                println!("Receiver after: {:.2} (+{:.2})", receiver_after as f64 / 1e6, delta as f64 / 1e6);
                assert!(delta >= amount, "❌ Not enough USDC received");
                println!("✅ Mixing complete!");
                return;
            }
        }

        println!("Still pending...");
    }

    panic!("❌ Mixing did not complete in time");
}
