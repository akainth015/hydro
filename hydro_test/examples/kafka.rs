fn main() {
    // [Leader]
    // If it already exists, delete the "financial_transactions" topic
    // Create a new topic named "financial_transactions" with 10 partitions.

    // Produce 1 million transactions into the financial_transactions topic, spread evenly
    // across the 10 partitions

    // [Consumers]
    // Once the cluster of consumers receives the go-ahead from the leader stream, it may then
    // begin to consume the financial transactions and compute the balance of each bank account
}
