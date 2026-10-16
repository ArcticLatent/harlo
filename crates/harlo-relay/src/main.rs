//! The harlo-relay program; see the library for what it does.

const HELP: &str = "harlo-relay: lets phones reach harlo from anywhere. Serves harlo's phone app and passes on
its end-to-end encrypted messages. Put HTTPS in front of it (Caddy, a Cloudflare Tunnel).

  --listen ADDR        where to listen (default 0.0.0.0:8080; HARLO_RELAY_LISTEN)
  --trust-forwarded    count connections by CF-Connecting-IP / X-Forwarded-For
                       (only behind a proxy; HARLO_RELAY_TRUST_FORWARDED=1)
  --per-ip N           connections from one address at once (default 64)
  --max-hosts N        harlos connected at once (default 10000)

The iOS app's notifications, sent for every harlo with the app's APNs key
(Apple only takes them signed with a key of the app's team):

  --apns-key FILE      the key's .p8 file (HARLO_RELAY_APNS_KEY)
  --apns-key-id ID     its Key ID (HARLO_RELAY_APNS_KEY_ID; else from the
                       file's name, AuthKey_<ID>.p8)
  --apns-team ID       the team's id (HARLO_RELAY_APNS_TEAM)
  --apns-topic ID      the app's bundle id (default com.arcticlatent.harlo)";

#[tokio::main]
async fn main() {
    let mut listen = std::env::var("HARLO_RELAY_LISTEN").unwrap_or_else(|_| "0.0.0.0:8080".to_owned());
    let mut config = harlo_relay::Config { trust_forwarded: std::env::var("HARLO_RELAY_TRUST_FORWARDED").is_ok_and(|v| v == "1"), ..Default::default() };
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let (mut key_file, mut key_id, mut team) = (env("HARLO_RELAY_APNS_KEY"), env("HARLO_RELAY_APNS_KEY_ID"), env("HARLO_RELAY_APNS_TEAM"));
    let mut topic = "com.arcticlatent.harlo".to_owned();
    let mut args = std::env::args().skip(1);
    let number = |v: Option<String>, flag: &str| -> usize {
        v.and_then(|v| v.parse().ok()).filter(|n| *n > 0).unwrap_or_else(|| {
            eprintln!("{flag} needs a number");
            std::process::exit(2)
        })
    };
    while let Some(a) = args.next() {
        match a.as_str() {
            "--listen" => listen = args.next().unwrap_or(listen),
            "--trust-forwarded" => config.trust_forwarded = true,
            "--per-ip" => config.limits.per_ip = number(args.next(), "--per-ip"),
            "--max-hosts" => config.limits.hosts = number(args.next(), "--max-hosts"),
            "--apns-key" => key_file = args.next(),
            "--apns-key-id" => key_id = args.next(),
            "--apns-team" => team = args.next(),
            "--apns-topic" => topic = args.next().unwrap_or(topic),
            "--help" | "-h" => {
                println!("{HELP}");
                return;
            }
            other => {
                eprintln!("unknown argument {other} (try --help)");
                std::process::exit(2);
            }
        }
    }
    if let Some(file) = key_file {
        let fail = |why: String| -> ! {
            eprintln!("harlo-relay: the APNs key: {why}");
            std::process::exit(2)
        };
        let p8 = std::fs::read_to_string(&file).unwrap_or_else(|e| fail(format!("can't read {file}: {e}")));
        // Apple names it AuthKey_<Key ID>.p8.
        let key_id = key_id.or_else(|| {
            std::path::Path::new(&file).file_stem()?.to_str()?.strip_prefix("AuthKey_").map(str::to_owned)
        }).unwrap_or_else(|| fail("--apns-key-id is missing".into()));
        let team = team.unwrap_or_else(|| fail("--apns-team is missing".into()));
        let credentials = harlo_apns::Credentials::new(&team, &key_id, &p8).unwrap_or_else(|e| fail(format!("{e:#}")));
        let client = harlo_apns::Client::new(credentials, env("HARLO_TEST_APNS_URL")).unwrap_or_else(|e| fail(format!("{e:#}")));
        eprintln!("harlo-relay: sending notifications for {topic} (key {key_id})");
        config.push = Some(harlo_relay::Push { client: std::sync::Arc::new(client), topic });
    }
    let listener = match tokio::net::TcpListener::bind(&listen).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("harlo-relay: can't listen on {listen}: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("harlo-relay listening on {}", listener.local_addr().map(|a| a.to_string()).unwrap_or(listen));
    let shutdown = async {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    };
    if let Err(e) = harlo_relay::serve(listener, config, shutdown).await {
        eprintln!("harlo-relay: {e}");
        std::process::exit(1);
    }
}
