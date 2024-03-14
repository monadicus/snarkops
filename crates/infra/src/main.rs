pub mod schema;

use crate::schema::timeline::NodeTarget;

fn main() {
    // let item_raw = "
    // version: storage.snarkos.testing.monadic.us/v1

    // name: test name
    // description: test description
    // ";

    // let item: ItemDocument =
    // serde_yaml::from_str(item_raw).expect("deserialize");

    // println!("{:#?}", item);
    // println!("\n{}", serde_yaml::to_string(&item).expect("serialize"));

    println!("{:#?}", serde_yaml::from_str::<NodeTarget>("all").unwrap())
}
