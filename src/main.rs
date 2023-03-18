#![feature(async_closure, let_chains)]
use std::{fmt::{Debug, Display}, collections::HashSet, sync::Arc};

use chrono::{DateTime, Utc, Duration};
use futures::executor::block_on;
use rocket::{Request, response::stream::{EventStream, Event}, Shutdown, State, fs::{FileServer, relative}, Rocket, Build, fairing::{self, AdHoc}, serde::json::Json};
use rocket_db_pools::{sqlx, Database, Connection};
use rocket_dyn_templates::{Template, handlebars::Handlebars};
use serde::{Serialize, Deserialize};
use sqlx::{FromRow, sqlite::SqliteArguments, Arguments, Row, pool::PoolConnection, Sqlite, Pool};
use tokio::{sync::{broadcast::{channel, error::RecvError, Receiver, Sender}, RwLock}, time::timeout};
use veloren_common::uuid::Uuid;

use crate::veloren::{env_key, run};

#[macro_use] extern crate rocket;

mod veloren;

#[get("/")]
async fn index(player_list: &State<PlayerList>, mut db: Connection<Db>) -> Template {
    #[derive(Serialize)]
    struct Player {
        alias: String,
        id: String,
        entry_id: String,
    }
    #[derive(Serialize, Default)]
    struct Context {
        players: Vec<Player>,
    }
    let mut context = Context::default();
    let ids = player_list.read().await.iter().copied().collect::<Vec<_>>();
    for id in ids {
        let alias =  sqlx::query_scalar::<_, String>("
            select alias
            from players
            where id = ?;
        ").bind(id).fetch_one(&mut *db).await.unwrap();
        context.players.push(Player {
            alias,
            id: format!("player-{id}"),
            entry_id: format!("player-entry-{id}"),
        });
    }
    Template::render("home", context)
}

#[catch(404)]
fn not_found(req: &Request) -> String {
    print!("{}", req);
    format!("Oh no! We couldn't find the requested path '{}'", req.uri())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(u32)]
pub enum MessageType {
    World,
    Tell,
    Faction,
}

impl From<&str> for MessageType {
    fn from(value: &str) -> Self {
        match value {
            "World" => MessageType::World,
            "Tell" => MessageType::Tell,
            "Faction" => MessageType::Faction,
            _ => panic!(),
        }
    }
}

impl Display for MessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            MessageType::World => "World",
            MessageType::Tell => "Tell",
            MessageType::Faction => "Faction",
        })
    }
}

#[derive(Debug)]
enum VelorenEventKind {
    Message {
        message: String,
        ty: MessageType,
    },
    Activity {
        online: bool,
    },
}

pub struct VelorenEvent {
    player_alias: String,
    player_uuid: Uuid,
    time: DateTime<Utc>,
    kind: VelorenEventKind,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Message {
    id: u32,
    player_id: u32,
    message: String,
    ty: MessageType,
    time: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Activity {
    player_id: u32,
    online: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum NetworkEvent {
    Message(Message),
    Activity(Activity),
}

#[get("/events")]
async fn events(queue: &State<Receiver<NetworkEvent>>, mut end: Shutdown) -> EventStream![] {
    let mut rx = queue.resubscribe();
    
    EventStream! {
        loop {
            let msg = rocket::tokio::select! {
                msg = rx.recv() => match msg {
                    Ok(msg) => msg,
                    Err(RecvError::Closed) => break,
                    Err(RecvError::Lagged(_)) => continue,
                },
                _ = &mut end => break,
            };

            yield Event::json(&msg);
        }
    }
}

#[post("/player_alias", data="<id>")]
async fn player_alias(mut db: Connection<Db>, id: &str) -> Option<String> {
    sqlx::query("select alias from players where id = ?").bind(id.parse::<u32>().ok()?)
        .fetch_one(&mut *db).await
        .and_then(|r| r.try_get(0))
        .ok()
}

#[derive(FromRow)]
struct DbMessage {
    id: u32,
    player_id: u32,
    time: DateTime<Utc>,
    content: String,
    ty: String,
}

impl From<DbMessage> for Message {
    fn from(msg: DbMessage) -> Self {
        Message {
            id: msg.id,
            player_id: msg.player_id,
            time: msg.time,
            message: msg.content,
            ty: MessageType::from(&*msg.ty),
        }
    }
}

#[post("/messages_before?<id>")]
async fn messages_before(mut db: Connection<Db>, id: Option<u32>) -> Json<Vec<Message>> {
    let messages = if let Some(id) = id {
        sqlx::query_as::<_, DbMessage>("
            select *
            from messages
            where id < ?
            order by id desc
            limit 50;
        ").bind(id).fetch_all(&mut *db)
    } else {
        sqlx::query_as::<_, DbMessage>("
            select *
            from messages
            order by id desc
            limit 50;
        ").bind(id).fetch_all(&mut *db)
    }
    .await.unwrap();
    
    Json(messages.into_iter().map(Message::from).collect())
}

#[post("/messages_after?<id>")]
async fn messages_after(mut db: Connection<Db>, id: Option<u32>) -> Json<Vec<Message>> {
    let messages = if let Some(id) = id {
        sqlx::query_as::<_, DbMessage>("
            select *
            from messages
            where id > ?
            order by id asc
            limit 50;
        ").bind(id).fetch_all(&mut *db)
    } else {
        sqlx::query_as::<_, DbMessage>("
            select *
            from messages
            order by id asc
            limit 50;
        ").bind(id).fetch_all(&mut *db)
    }
    .await.unwrap();
    
    Json(messages.into_iter().map(Message::from).collect())
}

#[post("/players")]
async fn player_list(player_list: &State<PlayerList>) -> Json<Vec<u32>> {
    Json(player_list.read().await.iter().copied().collect())
}

#[derive(Deserialize)]
struct MessageQuery {
    per_page: Option<u32>,
    page: Option<u32>,
    player_id: Option<u32>,
    after: Option<String>,
    before: Option<String>
}

#[post("/query_messages", data="<query>")]
async fn query_messages(mut db: Connection<Db>, query: Json<MessageQuery>) -> Json<Vec<Message>> {
    let mut args = SqliteArguments::default();
    let mut where_statements = Vec::new();
    let mut input_n = 1;
    if let Some(player_id) = query.player_id {
        where_statements.push(format!("player_id = ${input_n}"));
        input_n += 1;
        args.add(player_id);
    }
    let convert_dt = |dt: &str| Some(DateTime::<Utc>::from(DateTime::parse_from_rfc2822(&dt).ok()?));
    match (query.after.as_deref().and_then(convert_dt), query.before.as_deref().and_then(convert_dt)) {
        (Some(after), Some(before)) => {
            where_statements.push(format!("time between date(${input_n}) and date(${})", input_n + 1));
            input_n += 2;
            args.add(after);
            args.add(before);
        }
        (Some(after), None) => {
            where_statements.push(format!("date(time) > date(${input_n})"));
            input_n += 1;
            args.add(after);
        }
        (None, Some(before)) => {
            where_statements.push(format!("date(time) < date(${input_n})"));
            input_n += 1;
            args.add(before);
        }
        (None, None) => {}
    }
    let mut where_statements = where_statements.into_iter();
    let where_statement = where_statements.next();
    let where_statement = where_statement.map(|acc| where_statements.fold("where ".to_owned() + &acc, |mut acc, b| {
        acc.push_str(" and ");
        acc.push_str(&b);
        acc
    })).unwrap_or(String::new());
    let per_page = query.per_page.unwrap_or(50);
    args.add(query.per_page);

    args.add(query.page.unwrap_or(0) * per_page);
    let query = format!("select * from messages {where_statement} order by id desc limit ${input_n} offset ${}", input_n + 1);

    let messages = sqlx::query_as_with::<_, DbMessage, _>(&query, args).fetch_all(&mut *db).await.unwrap();
    
    Json(messages.into_iter().map(Message::from).collect())
}

#[post("/players?<alias>")]
async fn query_players(mut db: Connection<Db>, alias: Option<String>) -> Json<Vec<u32>> {
    let like = format!("%{}%", alias.as_deref().unwrap_or(""));
    let ids = sqlx::query_scalar("
        select id
        from players
        where alias like ?;
    ").bind(like).fetch_all(&mut *db).await.unwrap();

    Json(ids)
}

async fn query_playtime(db: &mut Connection<Db>, id: u32) -> (Duration, bool) {
    #[derive(FromRow)]
    struct Activity {
        time: DateTime<Utc>,
        online: bool,
    }
    sqlx::query_as::<_, Activity>("
        select time,online
        from activity
        where player_id = ?;
    ").bind(id).fetch_all(&mut **db).await.map(|activity| {
        let mut expects_online = true;
        let mut duration = Duration::zero();
        let mut start = Utc::now();
        for a in activity {
            if a.online == expects_online {
                if a.online {
                    start = a.time;
                } else {
                    duration = duration.checked_add(&(a.time - start)).unwrap();
                }
                
                expects_online = !expects_online;
            } else {
                rocket::error!("Expected online = {expects_online}");
            }
        }
        if !expects_online {
            duration = duration.checked_add(&(Utc::now() - start)).unwrap();
        }
        (duration, !expects_online)
    }).unwrap()
}

#[get("/user/<id>")]
async fn user_page(mut db: Connection<Db>, id: u32) -> Template {
    match sqlx::query_scalar::<_, String>("
        select alias
        from players
        where id = ?;
    ").bind(id).fetch_one(&mut *db).await {
        Ok(alias) => {
            #[derive(Serialize)]
            struct Context {
                alias: String,
                play_time: u64,
                online: bool,
            }

            let (pt, online) = query_playtime(&mut db, id).await;
            let context = Context {
                alias,
                play_time: pt.num_seconds() as u64,
                online,
            };

            Template::render("user", context)
        }
        Err(e) => {
            dbg!(e);
            return Template::render("user_not_found", ());
        }
    }

}

#[derive(Database)]
#[database("logs")]
struct Db(sqlx::SqlitePool);

async fn run_migrations(rocket: Rocket<Build>) -> fairing::Result {
    match Db::fetch(&rocket) {
        Some(db) => match sqlx::migrate!("db/logs/migrations").run(&**db).await {
            Ok(_) => {
                Ok(rocket)
            },
            Err(e) => {
                error!("Failed to initialize SQLx database: {}", e);
                Err(rocket)
            }
        }
        None => Err(rocket),
    }
}

pub fn customize_hbs(hbs: &mut Handlebars) {
    hbs.register_template_file("live-chat", "templates/live_chat.hbs").expect("valid HBS template");

    hbs.register_template_file("head", "templates/head.hbs").expect("valid HBS template");
}

type PlayerList = Arc<RwLock<HashSet<u32>>>;
async fn handle_database_msg(mut conn: PoolConnection<Sqlite>, msg: VelorenEvent, player_list: &PlayerList, sx: &Sender<NetworkEvent>) {
    let mut args = SqliteArguments::default();
    args.add(msg.player_uuid);
    args.add(msg.player_alias);
    let player_id = sqlx::query_scalar_with::<_, u32, _>(
        "
        insert or ignore into players (uuid, alias) values ($1, $2);
        select id from players where uuid = $1;
        ", args
    )
    .fetch_one(&mut conn)
    .await.unwrap();

    let mut args = SqliteArguments::default();
    args.add(player_id);
    args.add(msg.time);
    match msg.kind {
        VelorenEventKind::Message { message, ty } => {
            args.add(message.clone());
            args.add(ty.to_string());

            let id = sqlx::query_scalar_with::<_, u32, _>(
                "
                insert into messages (player_id, time, content, ty) values ($1, $2, $3, $4);
                select last_insert_rowid() as id;
                ", args
            ).fetch_one(&mut conn)
            .await.unwrap();

            let _ = sx.send(NetworkEvent::Message(Message {
                id: id,
                player_id: player_id,
                message,
                time: msg.time,
                ty,
            }));
        },
        VelorenEventKind::Activity { online } => {
            if online {
                player_list.write().await.insert(player_id);
            } else {
                player_list.write().await.remove(&player_id);
            }
            args.add(online);
            sqlx::query_with("
                insert into activity (player_id, time, online) values ($1, $2, $3);
            ", args).execute(&mut conn).await.unwrap();

            let _ = sx.send(NetworkEvent::Activity(Activity {
                player_id,
                online,
            }));
        },
    }

}


struct DbDrop {
    rx: tokio::sync::mpsc::Receiver<VelorenEvent>,
    sx: Sender<NetworkEvent>,
    pool: Pool<Sqlite>,
    player_list: PlayerList,
}

impl Drop for DbDrop {
    fn drop(&mut self) {
        block_on(async {
            while let Ok(Some(msg)) = timeout(std::time::Duration::from_millis(100), self.rx.recv()).await {
                if let Ok(conn) = self.pool.acquire().await {
                    handle_database_msg(conn, msg, &self.player_list, &self.sx).await
                }
            }
        });
    }
}

#[launch]
async fn rocket() -> _ {
    kankyo::init().unwrap();
    let (sx_db, rx_db) = tokio::sync::mpsc::channel::<VelorenEvent>(256);
    let (sx, rx) = channel::<NetworkEvent>(256);
    let player_list = PlayerList::default();
    rocket::build()
        .manage(rx)
        .manage(player_list.clone())
        .attach(Db::init())
        .attach(AdHoc::try_on_ignite("Logs db migrations", run_migrations))
        .attach(AdHoc::try_on_ignite("Route through database", |rocket| async {
            let pool = match Db::fetch(&rocket) {
                Some(pool) => pool.0.clone(),
                None => return Err(rocket),
            };

            rocket::tokio::task::spawn(async move {
                let mut db = DbDrop {
                    rx: rx_db,
                    pool,
                    player_list,
                    sx,
                };
                loop {
                    match db.rx.recv().await {
                        Some(msg) => {
                            if let Ok(conn) = db.pool.acquire().await {
                                handle_database_msg(conn, msg, &db.player_list, &db.sx).await
                            }
                        }
                        None => {
                            println!("No more messages");
                            break;
                        },
                    }
                }
            });

            Ok(rocket)
        }))
        .attach(AdHoc::on_liftoff("Veloren client", |rocket| {
            Box::pin(async move {
                let veloren_server = veloren_client::addr::ConnectionArgs::Tcp {
                    hostname: std::env::var("VELOREN_SERVER")
                        .expect("No environment variable 'VELOREN_SERVER' found."),
                    prefer_ipv6: false,
                };

                let veloren_username = env_key("VELOREN_USERNAME");
                let veloren_password = env_key("VELOREN_PASSWORD");
                let trusted_auth_server = env_key("VELOREN_TRUSTED_AUTH_SERVER");
                run(
                    veloren_server,
                    veloren_username,
                    veloren_password,
                    trusted_auth_server,
                    sx_db,
                    std::sync::Arc::new(rocket::tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap()),
                    rocket.shutdown(),
                );

                rocket::info!("Veloren-Common/Client version: {}", *veloren_common::util::DISPLAY_VERSION_LONG);
            })
        }))
        .attach(Template::custom(|engine| {
            customize_hbs(&mut engine.handlebars);
        }))
        .register("/", catchers!(not_found))
        .mount("/", routes![index, user_page])
        .mount("/api", routes![query_players, events, player_alias, messages_before, messages_after, query_messages, player_list])
        .mount("/static", FileServer::from(relative!("static")))
}