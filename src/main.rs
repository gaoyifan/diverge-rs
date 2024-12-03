use log::*;
use tokio::task;

mod conf;
mod diverge;
mod domain_map;
mod ip_map;
mod resolver;
mod udpd;

use conf::DivergeConf;
use diverge::Diverge;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

	let conf_fn = if std::env::args().len() < 2 {
		"diverge.conf".to_string()
	} else {
		std::env::args().nth(1).unwrap()
	};

	info!("read config from {}", &conf_fn);
	let conf_str = std::fs::read_to_string(conf_fn).unwrap();
	let conf: DivergeConf = conf_str.parse().unwrap();

	let diverge = Diverge::from(&conf);

	let local = task::LocalSet::new();
	local
		.run_until(udpd::udpd(conf.global.listen, diverge))
		.await;
	local.await;
}

// the real main running in a local set

#[cfg(test)]
mod tests {
	use super::*;
	use hickory_proto::op::Message;

	#[test]
	fn sizes() {
		println!("Duration: {}", std::mem::size_of::<std::time::Duration>());
		println!("Message: {}", std::mem::size_of::<Message>());
		println!(
			"TokioAsyncResolver: {}",
			std::mem::size_of::<hickory_resolver::TokioAsyncResolver>()
		);
		println!("Diverge: {}", std::mem::size_of::<Diverge>());
	}
}
