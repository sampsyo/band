// Passed as a global from the backend.
declare const BAND_ROOM_ID: string;

interface Message {
    body: string;
    ts: any;
};

window.addEventListener('DOMContentLoaded', async (event) => {
    const outEl = document.getElementById("messages")!;
    const outContainerEl = document.getElementById("output")!;
    const formEl = document.getElementById("send")! as HTMLFormElement;
    const msgEl = document.getElementById("sendMessage")! as HTMLInputElement;

    function addMessage(msg: Message, fresh: boolean) {
        const line = document.createElement("p");

        if (fresh) {
            line.classList.add("fresh");
            setTimeout(() => line.classList.add("done"), 0);
        }

        line.textContent = msg.body;
        outEl.appendChild(line);
        outContainerEl.scrollTop = 0;
    }

    let resp = await fetch(`/${BAND_ROOM_ID}/history`);
    let data = await resp.json();
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
        const body = msgEl.value;

        fetch(`/${BAND_ROOM_ID}/send`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify(body),
        });

        formEl.reset();
        event.preventDefault();
    });
});
