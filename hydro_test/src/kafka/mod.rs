use std::future::Future;

use hydro_lang::live_collections::boundedness::Boundedness;
use hydro_lang::live_collections::stream::{AtLeastOnce, ExactlyOnce, NoOrder, Ordering};
use hydro_lang::location::tick::{NoAtomic, NoTick};
use hydro_lang::location::Location;
use hydro_lang::prelude::*;
use rdkafka::message::OwnedMessage;
use rdkafka::producer::FutureProducer;

#[ctor::ctor]
fn init_rewrites() {
    stageleft::add_private_reexport(
        vec!["rdkafka", "producer", "future_producer"],
        vec!["rdkafka", "producer"],
    );
    stageleft::add_private_reexport(
        vec!["rdkafka", "consumer", "stream_consumer"],
        vec!["rdkafka", "consumer"],
    );
    stageleft::add_private_reexport(
        vec!["rdkafka", "message", "owned_message"],
        vec!["rdkafka", "message"],
    );
    stageleft::add_private_reexport(
        vec!["futures_util", "stream", "stream"],
        vec!["futures_util", "stream"],
    );
    stageleft::add_private_reexport(
        vec!["futures_util", "stream", "unfold"],
        vec!["futures_util", "stream"],
    );
}

/// Creates a Kafka `FutureProducer` singleton.
///
/// The topic will be created on the broker before the producer is returned.
/// This runs on the deployed host, so it works even when brokers are in a
/// private network unreachable from the local machine.
pub fn kafka_producer<'a, Loc>(
    location: &Loc,
    brokers: &'a str,
    security_protocol: &'a str,
    topic: &'a str,
    num_partitions: i32,
) -> Singleton<FutureProducer, Loc, Bounded>
where
    Loc: Location<'a> + NoTick + NoAtomic,
{
    location.singleton(q!({
        self::setup_topic_blocking(brokers, topic, num_partitions, security_protocol);
        rdkafka::config::ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("security.protocol", security_protocol)
            .create::<rdkafka::producer::FutureProducer>()
            .expect("Failed to create Kafka producer")
    }))
}

/// Consumes messages from a Kafka topic. Returns at-least-once, unordered delivery.
pub fn kafka_consumer<'a, Loc>(
    location: &Loc,
    brokers: &'a str,
    group_id: &'a str,
    topic: &'a str,
    security_protocol: &'a str,
) -> Stream<OwnedMessage, Loc, Bounded, NoOrder, AtLeastOnce>
where
    Loc: Location<'a> + NoTick + NoAtomic,
{
    location
        .singleton(q!({
            let consumer: rdkafka::consumer::StreamConsumer =
                rdkafka::config::ClientConfig::new()
                    .set("bootstrap.servers", brokers)
                    .set("group.id", group_id)
                    .set("auto.offset.reset", "earliest")
                    .set("security.protocol", security_protocol)
                    .create()
                    .expect("Failed to create Kafka consumer");
            rdkafka::consumer::Consumer::subscribe(&consumer, &[topic])
                .expect("Failed to subscribe to topic");
            std::sync::Arc::new(consumer)
        }))
        .into_stream()
        .flat_map_stream_blocking(q!(
            |consumer: std::sync::Arc<rdkafka::consumer::StreamConsumer>| {
                futures_util::stream::unfold(consumer, |consumer| async move {
                    loop {
                        match rdkafka::consumer::StreamConsumer::recv(&*consumer).await {
                            Ok(msg) => {
                                return Some((rdkafka::message::BorrowedMessage::detach(&msg), consumer));
                            }
                            Err(e) => {
                                eprintln!("Kafka consumer error: {}", e);
                                continue;
                            }
                        }
                    }
                })
            }
        ))
        .weaken_retries()
        .weaken_ordering()
}

/// Sends `(key, payload)` pairs to a Kafka topic.
pub fn dest_kafka<'a, Loc, Bound: Boundedness, Order: Ordering>(
    producer: Singleton<FutureProducer, Loc, Bounded>,
    input: Stream<(String, String), Loc, Bound, Order, ExactlyOnce>,
    topic: &'a str,
) where
    Loc: Location<'a>,
{
    input
        .cross_singleton(producer)
        .map(q!(
            |((key, payload), producer)| self::kafka_send(producer, topic, key, payload)
        ))
        .resolve_futures_blocking();
}

fn kafka_send(
    producer: FutureProducer,
    topic: &str,
    key: String,
    payload: String,
) -> impl Future<Output = ()> {
    let topic = topic.to_owned();
    async move {
        let record = rdkafka::producer::FutureRecord::to(&topic)
            .key(&key)
            .payload(&payload);
        producer
            .send(record, rdkafka::util::Timeout::Never)
            .await
            .expect("Failed to send message to Kafka");
    }
}

/// Admin helper: create a topic with the given number of partitions.
pub async fn setup_topic(brokers: &str, topic: &str, num_partitions: i32, security_protocol: &str) {
    use rdkafka::admin::{AdminClient, AdminOptions, NewTopic, TopicReplication};
    use rdkafka::config::ClientConfig;

    let admin: AdminClient<rdkafka::client::DefaultClientContext> = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("security.protocol", security_protocol)
        .create()
        .expect("Failed to create Kafka admin client");

    let new_topic = NewTopic::new(topic, num_partitions, TopicReplication::Fixed(2));
    admin
        .create_topics(&[new_topic], &AdminOptions::new())
        .await
        .expect("Failed to create Kafka topic");
}

/// Blocking version of [`setup_topic`] for use inside synchronous `q!()` blocks.
/// Uses the existing tokio runtime handle from the Hydro process.
pub fn setup_topic_blocking(brokers: &str, topic: &str, num_partitions: i32, security_protocol: &str) {
    let handle = tokio::runtime::Handle::current();
    let brokers = brokers.to_owned();
    let topic = topic.to_owned();
    let security_protocol = security_protocol.to_owned();
    // Spawn a separate thread to avoid calling block_on from within an async context.
    std::thread::spawn(move || {
        handle.block_on(setup_topic(&brokers, &topic, num_partitions, &security_protocol));
    })
    .join()
    .expect("Topic setup thread panicked");
}
