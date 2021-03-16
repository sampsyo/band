window.addEventListener('DOMContentLoaded', (event) => {
    const outEl = document.getElementById("output");
    const formEl = document.getElementById("send");
    const msgEl = document.getElementById("sendMessage");

    function addMessage(msg) {
        const line = document.createElement("p");
        line.textContent = msg;
        outEl.appendChild(line);
    }

    fetch('/history')
        .then((resp) => resp.json())
        .then((data) => {
            for (const msg of data) {
                addMessage(msg);
            }
        });

    const source = new EventSource("/chat");
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

        fetch('/send', {
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
