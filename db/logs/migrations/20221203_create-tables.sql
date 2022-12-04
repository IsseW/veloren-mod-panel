CREATE TABLE messages(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    player_id INTEGER,
    time DATETIME,
    content TEXT NOT NULL,
    ty TEXT NOT NULL
);

CREATE TABLE players(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    uuid TEXT NOT NULL UNIQUE,
    alias TEXT NOT NULL
);

CREATE TABLE activity(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    player_id INTEGER,
    time DATETIME,
    online BOOLEAN
)