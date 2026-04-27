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
pub fn kafka_producer<'a, Loc>(
    location: &Loc,
    brokers: &'a str,
) -> Singleton<FutureProducer, Loc, Bounded>
where
    Loc: Location<'a> + NoTick + NoAtomic,
{
    location.singleton(q!({
        rdkafka::config::ClientConfig::new()
            .set("bootstrap.servers", brokers)
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

/// Admin helper: delete topic if it exists, then create it with the given number of partitions.
pub async fn setup_topic(brokers: &str, topic: &str, num_partitions: i32) {
    use rdkafka::admin::{AdminClient, AdminOptions, NewTopic, TopicReplication};
    use rdkafka::config::ClientConfig;

    let admin: AdminClient<rdkafka::client::DefaultClientContext> = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .create()
        .expect("Failed to create Kafka admin client");

    let opts = AdminOptions::new();

    // Delete topic if it exists (ignore errors if it doesn't exist)
    let _ = admin.delete_topics(&[topic], &opts).await;
    // Brief pause to let deletion propagate
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let new_topic = NewTopic::new(topic, num_partitions, TopicReplication::Fixed(1));
    admin
        .create_topics(&[new_topic], &opts)
        .await
        .expect("Failed to create Kafka topic");
}
