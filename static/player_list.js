let player_list = document.getElementById("player-list");
let player_template = document.getElementById("player-entry-template");

function add_player(list, player_id) {
    var node = player_template.content.cloneNode(true);
    const id = 'player-entry-' + player_id;
    node.querySelector(".entry").id = id;
    node.querySelector(".entry .name").id = 'player-' + player_id;

    list.appendChild(node);
    get_player_alias(player_id).then(res => {
        var node = document.getElementById(id);
        node.querySelector(".entry .name").textContent = res;
    });
}

function remove_player(player_id) {
    let node = document.getElementById('player-entry-' + player_id);
    if (node != null) {
        node.remove();
    }
}

player_list.onch

document.addEventListener("activityrecv", function (ev) {
    if (ev.detail.online) {
        add_player(player_list, ev.detail.player_id);
    } else {
        remove_player(ev.detail.player_id);
    }
});

let player_search = document.getElementById("player-search");
let player_search_text = player_search.children[1];
let player_search_button = player_search.children[2];

let player_search_list = document.getElementById("player-search-list");

function search_player() {
    let search = player_search_text.value;
    player_search_text.disabled = true;
    player_search_button.disabled = true;

    if (search.length > 0) {
        search = "alias=" + search;
    }
    fetch("/api/players?" + search, {
        method: "POST",
    }).then(res => {
        res.json().then(res => {
            player_search_list.innerHTML = "";
            res.forEach(player_id => {
                add_player(player_search_list, player_id);
            });

            player_search_text.disabled = false;
            player_search_button.disabled = false;
        });
    });

}

player_search_text.addEventListener("keydown", (e) => {
    if (e.key == "Enter") {
        search_player();
    }
});

player_search_button.onclick = (e) => { search_player() };