use log::*;
use tokio::task;

use diverge::{
	conf::{Conf, DivergeConf},
	diverge::Diverge,
	udpd::udpd,
};

#[tokio::main(flavor = "current_thread")]
async fn main() {
	env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

	let conf_fn = if std::env::args().len() < 2 {
		"diverge.conf".to_string()
	} else {
		std::env::args().nth(1).unwrap()
	};

	info!("read config from {}", &conf_fn);
	let conf = DivergeConf::from_file(&conf_fn).unwrap();

	let diverge = Diverge::from(&conf);

	let local = task::LocalSet::new();
	local.run_until(udpd(conf.global.listen, diverge)).await;
	local.await;
}
