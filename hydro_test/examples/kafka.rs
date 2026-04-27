use std::sync::Arc;

use clap::{ArgAction, Parser};
use hydro_deploy::gcp::GcpNetwork;
use hydro_deploy::aws::NetworkResources;
use hydro_deploy::{AwsNetwork, Deployment, Host, HostTargetType, LinuxCompileType};
use hydro_lang::deploy::TrybuildHost;
use hydro_lang::live_collections::stream::{ExactlyOnce, TotalOrder};
use hydro_lang::location::Location;
use hydro_lang::nondet::nondet;
use hydro_lang::viz::config::GraphConfig;
use hydro_test::kafka::{dest_kafka, kafka_consumer, kafka_producer};
use stageleft::q;

type HostCreator = Box<dyn Fn(&mut Deployment) -> Arc<dyn Host>>;

const TOPIC_PREFIX: &str = "financial_transactions";
const NUM_PARTITIONS: i32 = 10;
const NUM_TRANSACTIONS: usize = 100_000;
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
    #[arg(long, default_value = "b-2.hydrotestinfrastructur.t43f0t.c8.kafka.us-west-2.amazonaws.com:9094,b-1.hydrotestinfrastructur.t43f0t.c8.kafka.us-west-2.amazonaws.com:9094")]
    brokers: String,

    /// Kafka security protocol (plaintext or SSL for MSK)
    #[arg(long, default_value = "SSL")]
    security_protocol: String,
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
        let region = "us-west-2";
        let network = AwsNetwork::new(
            region,
              Some(NetworkResources::new(
                  "vpc-0c5ff637759408d29".to_owned(),
                  "subnet-003906531a035a244".to_owned(),
                  "sg-09db2fe69913f6a76".to_owned(),
              )),
        );

        Box::new(move |deployment| -> Arc<dyn Host> {
            deployment
                .AwsEc2Host()
                .region(region)
                .instance_type("m7i.large")
                .ami("ami-055a9df0c8c9f681c") // Amazon Linux 2 (us-west-2)
                .network(network.clone())
                .target_type(HostTargetType::Linux(LinuxCompileType::Glibc))
                .add()
        })
    } else {
        let localhost = deployment.Localhost();
        Box::new(move |_| -> Arc<dyn Host> { localhost.clone() })
    };

    // Use a unique topic name per run to avoid stale messages from previous runs.
    let topic = format!("{}_{}", TOPIC_PREFIX, std::process::id());

    let mut flow = hydro_lang::compile::builder::FlowBuilder::new();
    let leader = flow.process::<Leader>();
    let consumers = flow.cluster::<Consumer>();

    // Leader: produce transactions spread across partitions.
    // Each transaction is (account_id, amount) serialized as key=account, value=amount.
    {
        let producer = kafka_producer(
            &leader,
            &args.brokers,
            &args.security_protocol,
            &topic,
            NUM_PARTITIONS,
        );
        let transactions = leader.source_iter(q!({
            (0..NUM_TRANSACTIONS).map(|i| {
                let account = format!("account_{}", i % 100);
                let amount = format!("{}", (i % 201) as i64 - 100); // range [-100, 100]
                (account, amount)
            })
        }));
        dest_kafka(producer, transactions, &topic);
        // Sentinel so the runner knows when producing is done.
        leader
            .source_iter(q!(std::iter::once("PRODUCE_DONE".to_string())))
            .for_each(q!(|msg| println!("{}", msg)));
    }

    // Consumers: read from topic and compute per-account balances.
    {
        let messages = kafka_consumer(&consumers, &args.brokers, "kafka_example_consumers", &topic, &args.security_protocol)
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
            .for_each(q!(|(account, amount)| {
                println!("{}: {}", account, amount);
            }));
    }

    // Extract the IR BEFORE the builder is consumed by deployment methods
    let built = flow.finalize();

    // Generate graph visualizations based on command line arguments
    if built.generate_graph(&args.graph)?.is_some() {
        return Ok(());
    }

    // Now use the built flow for deployment with optimization
    let nodes = built
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

    // Subscribe to stdout from all deployed nodes and count messages.
    let start = std::time::Instant::now();
    let total = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (done_tx, mut done_rx) = tokio::sync::mpsc::channel::<()>(1);
    let (produce_done_tx, produce_done_rx) = tokio::sync::oneshot::channel::<()>();
    {
        use hydro_lang::deploy::DeployCrateWrapper;

        let leader_node = nodes.get_process(&leader);
        let mut leader_out = leader_node.stdout();
        let produce_done_tx = std::sync::Mutex::new(Some(produce_done_tx));
        tokio::spawn(async move {
            while let Some(line) = leader_out.recv().await {
                if line.trim() == "PRODUCE_DONE" {
                    if let Some(tx) = produce_done_tx.lock().unwrap().take() {
                        let _ = tx.send(());
                    }
                } else {
                    println!("[Leader] {line}");
                }
            }
        });

        for (i, member) in nodes.get_cluster(&consumers).members().into_iter().enumerate() {
            let mut member_out = member.stdout();
            let total = total.clone();
            let done_tx = done_tx.clone();
            tokio::spawn(async move {
                while let Some(_line) = member_out.recv().await {
                    let t = total.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    if t % 10_000 == 0 {
                        println!("[Consumer {i}] ... {t} total messages consumed so far");
                    }
                    if t >= NUM_TRANSACTIONS {
                        let _ = done_tx.send(()).await;
                        return;
                    }
                }
            });
        }
    }
    drop(done_tx);

    println!("Running Kafka financial transactions example ({NUM_TRANSACTIONS} messages)...");

    let _ = produce_done_rx.await;
    let produce_elapsed = start.elapsed();
    println!(
        "Produce: {NUM_TRANSACTIONS} messages in {:.2?} ({:.0} msgs/sec)",
        produce_elapsed,
        NUM_TRANSACTIONS as f64 / produce_elapsed.as_secs_f64()
    );

    done_rx.recv().await;
    let total_elapsed = start.elapsed();
    let consume_elapsed = total_elapsed - produce_elapsed;
    println!(
        "Consume: {NUM_TRANSACTIONS} messages in {:.2?} ({:.0} msgs/sec)",
        consume_elapsed,
        NUM_TRANSACTIONS as f64 / consume_elapsed.as_secs_f64()
    );
    println!(
        "Total:   {:.2?}",
        total_elapsed,
    );
    Ok(())
}
