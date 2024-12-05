use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
pub struct CliArgs {
	#[command(subcommand)]
	pub cmd: Cmd,
}

#[derive(Args)]
pub struct QArgs {
	#[arg(short, long, default_value = "udp")]
	pub proto: String,

	#[arg(long)]
	pub port: Option<u16>,

	#[arg(short, long)]
	pub tls_dns_name: Option<String>,

	#[arg(long)]
	pub proxy: Option<String>,

	#[arg(long, default_value_t = 1)]
	pub repeat: usize,

	#[arg(long, default_value_t = 1.0)]
	pub interval: f32,

	#[arg(long, default_value_t = 1.0)]
	pub backoff: f32,

	pub server: String,

	pub name: String,

	#[arg(default_value = "A")]
	pub qtype: String,

	// hickory_resolver lookup doesn't support qclass anyway
	#[arg(default_value = "IN")]
	pub qclass: String,
}

#[derive(Subcommand)]
pub enum Cmd {
	Query(QArgs),
	Proxy { listen: String, origin: String },
}
