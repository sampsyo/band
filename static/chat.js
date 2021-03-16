const outEl = document.getElementById("output");

const source = new EventSource("/chat");
source.addEventListener('open', (event) => {
    console.log("open", event);
});
source.addEventListener('error', (event) => {
    console.log("error", event);
});

source.addEventListener('message', (event) => {
    console.log("message", event);
    const line = document.createElement("p");
    line.textContent = event.data;
    outEl.appendChild(line);
});
