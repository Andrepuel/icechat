use icechat::database::{DatabaseBase, Message};
use uuid::Uuid;

fn main() {
    let mut doc = DatabaseBase::zero();
    println!("msgs: {:#?}", doc.list_messages().collect::<Vec<_>>());
    doc.add_message(Message {
        from: Uuid::new_v4(),
        content: "Hello world!".to_string(),
    });
    println!("msgs: {:#?}", doc.list_messages().collect::<Vec<_>>());
}
