use cargo_packager_updater::{Config, Update, check_update, semver::Version, url::Url};

const UPDATE_ENDPOINT: &str =
    "https://github.com/Envl/paperoll/releases/latest/download/latest.json";
const UPDATE_PUBLIC_KEY: &str = include_str!("../resources/update.pub");

pub fn check() -> anyhow::Result<Option<Update>> {
    let config = Config {
        endpoints: vec![Url::parse(UPDATE_ENDPOINT)?],
        pubkey: UPDATE_PUBLIC_KEY.to_string(),
        ..Default::default()
    };
    let current_version = Version::parse(env!("CARGO_PKG_VERSION"))?;
    check_update(current_version, config).map_err(Into::into)
}

pub fn install(update: Update) -> anyhow::Result<()> {
    update.download_and_install().map_err(Into::into)
}
