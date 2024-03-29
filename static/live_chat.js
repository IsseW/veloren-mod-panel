let message_template = document.getElementById("message");

let messages_div = document.getElementById("messages");

var newest_message = null;

messages_div.onscroll = function () {
  if (messages_div.scrollTop == 0) {
    var element = messages_div.children[1];
    if (element == null) {
      fetch("/api/messages_before", {
        method: "POST",
      }).then(res => {
        res.json().then(res => {
          res.forEach(add_message_back);
        });
      });
    } else {
      let id = element.id.substring(4);
      fetch("/api/messages_before?id=" + id, {
        method: "POST",
      }).then(res => {
        res.json().then(res => {
          res.forEach(add_message_back);
        });
      });
    }
  } else if (messages_div.scrollTop == messages_div.scrollHeight - messages_div.offsetHeight) {
    var element = messages_div.children[messages_div.children.length - 1];
    if (element == null) {
      fetch("/api/messages_after", {
        method: "POST",
      }).then(res => {
        res.json().then(res => {
          res.forEach(add_message_front);
        });
      });
    } else {
      let id = element.id.substring(4);
      if (id != newest_message) {
        fetch("/api/messages_after?id=" + id, {
          method: "POST",
        }).then(res => {
          res.json().then(res => {
            res.forEach(add_message_front);
          });
        });
      }
    }
  } 
};


messages_div.style.width = window.localStorage.getItem("messages-size-width");
messages_div.style.height = window.localStorage.getItem("messages-size-height");
messages_div.scrollTop = (messages_div.scrollHeight - messages_div.clientHeight);
const observer = new ResizeObserver(function (res) {
  res.forEach(function (res) {
    window.localStorage.setItem(res.target.id + "-size-width", messages_div.style.width);
    window.localStorage.setItem(res.target.id + "-size-height", messages_div.style.height);
  });
});
observer.observe(messages_div);

dragElement(document.getElementById("message-box"));

function load_recent() {
  fetch("/api/messages_before", {
    method: "POST",
  }).then(res => {
    res.json().then(res => {
      if (res.length > 0) {
        newest_message = res[0].id;
        res.forEach(add_message_back);
      }
    });
  });
}

const clamp = (number, min, max) =>
   Math.max(min, Math.min(number, max));

function dragElement(elmnt) {
  elmnt.style.top = window.localStorage.getItem(elmnt.id + "-position-top");
  elmnt.style.left = window.localStorage.getItem(elmnt.id + "-position-left");
  var pos1 = 0, pos2 = 0, pos3 = 0, pos4 = 0;
  if (document.getElementById(elmnt.id + "header")) {
    // if present, the header is where you move the DIV from:
    document.getElementById(elmnt.id + "header").onmousedown = dragMouseDown;
  } else {
    // otherwise, move the DIV from anywhere inside the DIV:
    elmnt.onmousedown = dragMouseDown;
  }

  function dragMouseDown(e) {
    e = e || window.event;
    e.preventDefault();
    // get the mouse cursor position at startup:
    pos3 = e.clientX;
    pos4 = e.clientY;
    document.onmouseup = closeDragElement;
    // call a function whenever the cursor moves:
    document.onmousemove = elementDrag;
  }

  function elementDrag(e) {
    e = e || window.event;
    e.preventDefault();
    // calculate the new cursor position:
    pos1 = pos3 - e.clientX;
    pos2 = pos4 - e.clientY;
    pos3 = e.clientX;
    pos4 = e.clientY;
    // set the element's new position:
    var root = document.getElementsByTagName('body');
    var top = clamp((elmnt.offsetTop - pos2), 0, window.innerHeight - elmnt.offsetHeight);
    var left = clamp((elmnt.offsetLeft - pos1), 0, window.innerWidth - elmnt.offsetWidth);
    elmnt.style.top = top + "px";
    elmnt.style.left = left + "px";
    window.localStorage.setItem(elmnt.id + "-position-top", elmnt.style.top);
    window.localStorage.setItem(elmnt.id + "-position-left", elmnt.style.left);
  }

  window.addEventListener("resize", function () {
    var root = document.getElementsByTagName('body');
    var top = clamp((elmnt.offsetTop - pos2), 0, window.innerHeight - elmnt.offsetHeight);
    var left = clamp((elmnt.offsetLeft - pos1), 0, window.innerWidth - elmnt.offsetWidth);
    elmnt.style.top = top + "px";
    elmnt.style.left = left + "px";
    window.sessionStorage.setItem("chat-position-top", elmnt.style.top);
    window.sessionStorage.setItem("chat-position-left", elmnt.style.left);
  });

  function closeDragElement() {
    // stop moving when mouse button is released:
    document.onmouseup = null;
    document.onmousemove = null;
  }
}

function ty_color(ty) {
  if (ty == "World") {
    return `#f0f0f0`;
  }
  if (ty == "Tell") {
    return `#d90166`;
  }
  if (ty == "Faction") {
    return `#008000`
  }

  return `#f0f0f0`;
}

function get_player_alias(id) {
  return new Promise((resolve, reject) => {
    let storage_id = 'cached_player_' + id;
    var alias = window.sessionStorage.getItem(storage_id);
    if (alias == null) {
      fetch('/api/player_alias', {
        method: "POST",
        body: id,
      }).then(res => {
        res.text().then(res => {
          window.sessionStorage.setItem(storage_id, res);
          resolve(res);
        }).catch(reason => {
          reject(reason);
        });
      }).catch(reason => {
        reject(reason);
      });
    } else {
      resolve(alias);
    }
  });
}

function add_message(msg, add_func) {
  
  var node = message_template.content.cloneNode(true);
  const id = 'msg-' + msg.id;
  node.querySelector(".message").id = id;

  var date = new Date(msg.time);

  node.querySelector(".message .time").textContent = date.toLocaleTimeString();

  node.querySelector(".message .name").id = "player-" + msg.player_id;
  node.querySelector(".message .text").textContent = msg.message;
  node.querySelector(".message .text").style.color = ty_color(msg.ty);

  add_func(node);
  get_player_alias(msg.player_id).then(res => {
    var node = document.getElementById(id);
    node.querySelector(".message .name").textContent = res;
  });
}

function add_message_front(msg) {
  add_message(msg, function (node) {
    messages_div.appendChild(node);
  });
}

function clear_chat() {
  messages_div.innerHTML = '';
}

var selected = null;

document.addEventListener("click", function (ev) {
  let target = ev.target;
  if (target.classList.contains("name")) {
    window.location.href = '/user/' + target.id.substring("player-".length);
  } else if (target.classList.contains("goto")) {
    if (selected != null) {
      messages_div.querySelector('#' + selected).classList.remove('selected');
    }

    let elem_id = target.parentElement.id;
    let element = messages_div.querySelector('#' + elem_id);
    if (element == null) {
      clear_chat();
      let id = elem_id.substring(4);
      fetch("/api/messages_before?id=" + (+id + 25), {
        method: "POST",
      }).then(res => {
        res.json().then(res => {
          res.forEach(add_message_back);
          let element = messages_div.querySelector('#' + elem_id);
          element.classList.add('selected');
          element.scrollIntoView({
              behavior: 'auto',
              block: 'center',
              inline: 'center'
          });
        });
      });
    } else {
      element.classList.add('selected');
      element.scrollIntoView({
          behavior: 'auto',
          block: 'center',
          inline: 'center'
      });
    }
    selected = elem_id;
  }
});

document.getElementById('goto-bottom').onclick = function (ev) {
  let element = messages_div.children[messages_div.children.length - 1];
  if (element == null || element.id.substring(4) != newest_message) {
    clear_chat();
    load_recent();
  }
  messages_div.scrollTop = messages_div.scrollHeight - messages_div.offsetHeight;
};

function add_message_back(msg) {
  var scroll_bottom = (messages_div.scrollHeight - messages_div.clientHeight) - messages_div.scrollTop;
  add_message(msg, function (node) {
    if (messages_div.children.length > 1) {
      messages_div.insertBefore(node, messages_div.children[1]);
    } else {
      messages_div.appendChild(node);
    }
  });
  messages_div.scrollTop = (messages_div.scrollHeight - messages_div.clientHeight) - scroll_bottom;
}

document.addEventListener('messagerecv', function (ev) {
  newest_message = ev.detail.id;
  var element = messages_div.children[messages_div.children.length - 1];
  if (element == null || element.id.substring(4) == ev.detail.id - 1) {
    var is_at_bottom = (messages_div.scrollHeight - messages_div.clientHeight) - messages_div.scrollTop < 10;
    add_message_front(ev.detail);
    if (is_at_bottom) {
      messages_div.scrollTop = messages_div.scrollHeight - messages_div.clientHeight;
    }
  }
});

// Subscribe to the event source at `uri` with exponential backoff reconnect.
function subscribe(uri) {
  var retryTime = 1;

  function connect(uri) {
    const events = new EventSource(uri);

    events.addEventListener("message", (ev) => {
      const msg = JSON.parse(ev.data);
      if (msg.Activity != null) {
        var evt = new CustomEvent('activityrecv', {
          detail: msg.Activity,
        });
      }
      if (msg.Message != null) {
        var evt = new CustomEvent('messagerecv', {
          detail: msg.Message,
        });
      }
      document.dispatchEvent(evt);
    });

    events.addEventListener("open", () => {
      console.log(`connected to event stream at ${uri}`);
      retryTime = 1;
    });

    events.addEventListener("error", () => {
      events.close();

      let timeout = retryTime;
      retryTime = Math.min(64, retryTime * 2);
      console.log(`connection lost. attempting to reconnect in ${timeout}s`);
      setTimeout(() => connect(uri), (() => timeout * 500)());
    });
  }

  connect(uri);
}
const stylesheet = document.styleSheets[0];
let time_class;

for (let i = 0; i < stylesheet.cssRules.length; i++) {
  if (stylesheet.cssRules[i].selectorText === '.time') {
    time_class = stylesheet.cssRules[i];
  }
}

document.getElementById('toggle-time').onclick = function (ev) {
  if (time_class.style.getPropertyValue('display') == 'none') {
    time_class.style.setProperty('display', 'inline');
  } else {
    time_class.style.setProperty('display', 'none');
  }
};

load_recent();

subscribe("/api/events");