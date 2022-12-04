
let chat_log_template = document.getElementById("chat-log-message");

let chat_log_div = document.getElementById("chat-log");

const self_id = parseInt(window.location.href.substring(window.location.href.lastIndexOf('/') + 1));
fetch("/api/query_messages", {
    method: "POST",
    body: JSON.stringify({
        per_page: 100,
        page: 0,
        player_id: self_id,
    })
}).then(res => {
    res.json().then(res => {
        res.forEach(msg => {
            add_chat_log_back(msg);
        });
    });
});

function add_chat_log(msg, add_func) {
  var is_at_bottom = (chat_log_div.scrollHeight - chat_log_div.clientHeight) - chat_log_div.scrollTop < 10;
  
  var node = chat_log_template.content.cloneNode(true);
  const id = 'msg-' + msg.id;
  node.querySelector(".message").id = id;

  var date = new Date(msg.time);

  node.querySelector(".message .time-log").textContent = date.toLocaleTimeString();

  node.querySelector(".message .text").textContent = msg.message;
  node.querySelector(".message .text").style.color = ty_color(msg.ty);

  add_func(node);

  if (is_at_bottom) {
    chat_log_div.scrollTop = chat_log_div.scrollHeight - chat_log_div.clientHeight;
  }
}

function add_chat_log_front(msg) {
  add_chat_log(msg, function (node) {
    chat_log_div.appendChild(node);
  });
}

function add_chat_log_back(msg) {
  var scroll_bottom = (chat_log_div.scrollHeight - chat_log_div.clientHeight) - chat_log_div.scrollTop;
  add_chat_log(msg, function (node) {
    if (chat_log_div.children.length > 1) {
      chat_log_div.insertBefore(node, chat_log_div.children[1]);
    } else {
      chat_log_div.appendChild(node);
    }
  });
  chat_log_div.scrollTop = (chat_log_div.scrollHeight - chat_log_div.clientHeight) - scroll_bottom;
}


document.addEventListener('messagerecv', function (ev) {
    let msg = ev.detail;
    if (self_id == msg.player_id) {
        add_chat_log_front(msg);
    }
});