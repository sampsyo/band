window.addEventListener('DOMContentLoaded', async (event) => {
    const outEl = document.getElementById("messages")!;
    const formEl = document.getElementById("send")! as HTMLFormElement;
    const msgEl = document.getElementById("sendMessage")! as HTMLInputElement;

    function addMessage(msg: string) {
        const line = document.createElement("p");
        line.textContent = msg;
        outEl.appendChild(line);
    }

    let resp = await fetch('/TODO/history');
    let data = await resp.json();
    for (const msg of data) {
        addMessage(msg);
    }

    const source = new EventSource("/TODO/chat");
    source.addEventListener('open', (event) => {
        console.log("open", event);
    });
    source.addEventListener('error', (event) => {
        console.log("error", event);
    });
    source.addEventListener('message', (event) => {
        console.log("message", event);
        addMessage(event.data);
    });

    formEl.addEventListener('submit', (event) => {
        const body = msgEl.value;

        fetch('/TODO/send', {
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
