use mattermost_api::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
        std::env::var("HOME").unwrap() + "/.config/io.gitlab.akergez.Matterfast/session.json",
    )?)?;
    let c = Client::new(cfg["server"].as_str().unwrap())?;
    c.set_token(cfg["token"].as_str().unwrap().to_string());

    let me = c.me().await?;
    let teams = c.my_teams().await?;
    let team = &teams[0];
    let crt = true;
    let inbox = c.my_threads(&team.id, false, 5).await?;
    println!("threads in inbox: {}", inbox.threads.len());

    for t in inbox.threads.iter().take(3) {
        let list = c.post_thread(&t.id, crt).await?;
        println!(
            "\n--- thread {} (reply_count={}) ---",
            &t.id[..6],
            t.reply_count
        );
        println!("order as the server sent it:");
        for (i, id) in list.order.iter().enumerate() {
            let p = list.posts.get(id);
            println!(
                "  [{i}] id={} create_at={} root={}",
                &id[..6],
                p.map(|p| p.create_at).unwrap_or(-1),
                p.map(|p| if p.root_id.is_empty() {
                    "-"
                } else {
                    &p.root_id[..6]
                })
                .unwrap_or("?"),
            );
        }
    }
    let _ = me;
    Ok(())
}
