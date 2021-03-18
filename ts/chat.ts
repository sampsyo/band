// Passed as a global from the backend.
declare const BAND_ROOM_ID: string;

interface OutgoingMessage {
    body: string;
    user: string;
}

interface Message extends OutgoingMessage {
    ts: string;
}

async function send(msg: OutgoingMessage) {
    await fetch(`/${BAND_ROOM_ID}/send`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify(msg),
    });
}

window.addEventListener('DOMContentLoaded', async (event) => {
    const outEl = document.getElementById("messages")!;
    const outContainerEl = document.getElementById("output")!;
    const formEl = document.getElementById("send")! as HTMLFormElement;
    const msgEl = document.getElementById("sendMessage")! as HTMLInputElement;

    let username: string = "anonymous";

    function addMessage(msg: Message, fresh: boolean) {
        const line = document.createElement("p");
        if (fresh) {
            line.classList.add("fresh");
            setTimeout(() => line.classList.add("done"), 0);
        }

        const user = document.createElement("span");
        user.classList.add("user");
        line.appendChild(user);
        user.textContent = `${msg.user}:`;

        const body = document.createElement("span");
        body.classList.add("body");
        line.appendChild(body);
        body.textContent = msg.body;

        outEl.appendChild(line);
        outContainerEl.scrollTop = 0;
    }

    const resp = await fetch(`/${BAND_ROOM_ID}/history`);
    const data = await resp.json();
    for (const msg of data) {
        addMessage(msg, false);
    }

    const source = new EventSource(`/${BAND_ROOM_ID}/chat`);
    source.addEventListener('open', (event) => {
        console.log("open", event);
    });
    source.addEventListener('error', (event) => {
        console.log("error", event);
    });
    source.addEventListener('message', (event) => {
        console.log("message", event);
        addMessage(JSON.parse(event.data), true);
    });

    formEl.addEventListener('submit', (event) => {
        const text = msgEl.value;

        if (text.startsWith('/name')) {
            // Update username.
            username = text.split(' ')[1];
        } else {
            // Fire and forget; no need to await.
            send({
                body: text,
                user: username,
            });
        }

        formEl.reset();
        event.preventDefault();
    });
});
