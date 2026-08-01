use cosmic_connect::app::CosmicConnect;

fn main() -> cosmic::iced::Result {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    cosmic::applet::run::<CosmicConnect>(())?;

    Ok(())
}
