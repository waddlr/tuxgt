use vergen::{BuildBuilder, Emitter};

fn main() -> anyhow::Result<()> {
    Emitter::default()
        .add_instructions(&BuildBuilder::default().build_date(true).build()?)?
        .emit()
}
