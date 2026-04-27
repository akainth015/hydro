use std::collections::HashMap;
use std::sync::Arc;

use clap::{ArgAction, Parser};
use hydro_deploy::gcp::GcpNetwork;
use hydro_deploy::{AwsNetwork, Deployment, Host};
use hydro_lang::deploy::TrybuildHost;
use hydro_lang::live_collections::stream::{ExactlyOnce, TotalOrder};
use hydro_lang::location::Location;
use hydro_lang::nondet::nondet;
use hydro_lang::viz::config::GraphConfig;
use hydro_test::kafka::{dest_kafka, kafka_consumer, kafka_producer, setup_topic};
use stageleft::q;

type HostCreator = Box<dyn Fn(&mut Deployment) -> Arc<dyn Host>>;

const TOPIC: &str = "financial_transactions";
const NUM_PARTITIONS: i32 = 10;
const NUM_TRANSACTIONS: usize = 1_000_000;
const NUM_CONSUMERS: usize = 3;

// cargo run -p hydro_test --example kafka --features kafka -- --brokers 'localhost:9092'
#[derive(Parser, Debug)]
#[command(group(
    clap::ArgGroup::new("cloud")
        .args(&["gcp", "aws"])
        .multiple(false)
))]
struct Args {
    #[clap(flatten)]
    graph: GraphConfig,

    /// Use GCP for deployment (provide project name)
    #[arg(long)]
    gcp: Option<String>,

    /// Use AWS, make sure credentials are set up
    #[arg(long, action = ArgAction::SetTrue)]
    aws: bool,

    /// Kafka bootstrap servers
    #[arg(long, default_value = "localhost:9092")]
    brokers: String,
}

enum Leader {}
enum Consumer {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut deployment = Deployment::new();

    let create_host: HostCreator = if let Some(project) = &args.gcp {
        let network = GcpNetwork::new(project, None);
        let project = project.clone();

        Box::new(move |deployment| -> Arc<dyn Host> {
            deployment
                .GcpComputeEngineHost()
                .project(&project)
                .machine_type("e2-micro")
                .image("debian-cloud/debian-11")
                .region("us-west1-a")
                .network(network.clone())
                .add()
        })
    } else if args.aws {
        let region = "us-east-1";
        let network = AwsNetwork::new(region, None);

        Box::new(move |deployment| -> Arc<dyn Host> {
            deployment
                .AwsEc2Host()
                .region(region)
                .instance_type("t3.micro")
                .ami("ami-0e95a5e2743ec9ec9") // Amazon Linux 2
                .network(network.clone())
                .add()
        })
    } else {
        let localhost = deployment.Localhost();
        Box::new(move |_| -> Arc<dyn Host> { localhost.clone() })
    };

    // [Leader] Setup topic and produce transactions
    setup_topic(&args.brokers, TOPIC, NUM_PARTITIONS).await;
    println!("Topic '{}' created with {} partitions", TOPIC, NUM_PARTITIONS);

    let mut flow = hydro_lang::compile::builder::FlowBuilder::new();
    let leader = flow.process::<Leader>();
    let consumers = flow.cluster::<Consumer>();

    // Leader: produce 1M transactions spread across 10 partitions.
    // Each transaction is (account_id, amount) serialized as key=account, value=amount.
    {
        let producer = kafka_producer(&leader, &args.brokers);
        let transactions = leader.source_iter(q!({
            (0..NUM_TRANSACTIONS).map(|i| {
                let account = format!("account_{}", i % 100);
                let amount = format!("{}", (i % 201) as i64 - 100); // range [-100, 100]
                (account, amount)
            })
        }));
        dest_kafka(producer, transactions, TOPIC);
    }

    // Consumers: read from topic and compute per-account balances.
    {
        let messages = kafka_consumer(&consumers, &args.brokers, "kafka_example_consumers", TOPIC)
            .assume_ordering::<TotalOrder>(
                nondet!(/** Safe: side effect is only printing final balances. */),
            )
            .assume_retries::<ExactlyOnce>(
                nondet!(/** Safe: side effect is only printing final balances. */),
            )
            .filter_map(q!(|msg| {
                let key = rdkafka::Message::key(&msg)
                    .map(|k| String::from_utf8_lossy(k).to_string())?;
                let value = rdkafka::Message::payload(&msg)
                    .map(|v| String::from_utf8_lossy(v).to_string())?;
                let amount: i64 = value.parse().ok()?;
                Some((key, amount))
            }))
            .fold(
                q!(|| HashMap::<String, i64>::new()),
                q!(|balances, (account, amount)| {
                    *balances.entry(account).or_insert(0) += amount;
                }),
            );

        messages.into_stream().for_each(q!(|balances: HashMap<String, i64>| {
            println!("Final balances ({} accounts):", balances.len());
            let mut sorted: Vec<_> = balances.into_iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            for (account, balance) in sorted.iter().take(10) {
                println!("  {}: {}", account, balance);
            }
            if sorted.len() > 10 {
                println!("  ... and {} more accounts", sorted.len() - 10);
            }
        }));
    }

    // Extract the IR BEFORE the builder is consumed by deployment methods
    let built = flow.finalize();

    // Generate graph visualizations based on command line arguments
    built.generate_graph_with_config(&args.graph, None)?;

    // If we're just generating a graph file, exit early
    if args.graph.should_exit_after_graph_generation() {
        return Ok(());
    }

    // Now use the built flow for deployment with optimization
    let _nodes = built
        .with_default_optimize()
        .with_process(
            &leader,
            TrybuildHost::new(create_host(&mut deployment))
                .features(vec!["kafka".to_owned()]),
        )
        .with_cluster(
            &consumers,
            (0..NUM_CONSUMERS).map(|_| {
                TrybuildHost::new(create_host(&mut deployment))
                    .features(vec!["kafka".to_owned()])
            }),
        )
        .deploy(&mut deployment);

    deployment.deploy().await.unwrap();
    deployment.start().await.unwrap();

    println!("Running Kafka financial transactions example...");
    println!("Press Ctrl+C to stop.");

    tokio::signal::ctrl_c().await.unwrap();
    Ok(())
}
