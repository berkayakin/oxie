// dep: tokio = { version = "1", features = ["full"] }
// dep: serde = { version = "1", features = ["derive"] }
// dep: serde_json = "1"

import serde::{Serialize, Deserialize}
import std::time::Duration

@derive(Serialize, Deserialize)
data class Order(id: u32, item: String, qty: u32, note: String? = null)

async fun fetch_price(item: &str): f64 {
    tokio.time.sleep(Duration.from_millis(10)).await
    when (item) {
        "apple" -> 0.5
        "pear" -> 0.75
        else -> 1.0
    }
}

async fun total(orders: &[Order]): f64 {
    var sum = 0.0
    for (o in orders) {
        val price = await fetch_price(&o.item)
        sum += price * o.qty as f64
    }
    sum
}

@tokio::main
async fun main() throws {
    val json = """[{"id":1,"item":"apple","qty":4},{"id":2,"item":"pear","qty":2,"note":"ripe"}]"""
    val orders: Vec<Order> = serde_json.from_str(json)?
    println("parsed ${orders.len()} orders, second note: ${orders[1].note:?}")
    val handles: Vec<_> = (1..=3).map { i -> tokio.spawn(async move { i * 10 }) }.collect()
    var results = vec[]
    for (h in handles) results.push(h.await?)
    println("spawned: ${results:?}")
    println("total: ${total(&orders).await:.2}")
    val out = serde_json.to_string(&Order(3, "kiwi".to_string(), 1))?
    println(out)
}
