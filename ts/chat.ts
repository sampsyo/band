// Passed as a global from the backend.
declare const BAND_ROOM_ID: string;

const USERNAME_CMD = '/name';
const DEFAULT_USERNAME = 'anonymous';

interface Message {
    body: string;
    user: string;
    ts: string;
    id: string;
    votes: number;
}

interface VoteChange {
    message: string;
    delta: number;
}

interface SystemMessage {
    body: string;
    system: true;
}

class Client {
    session: string | undefined;

    constructor(
        public readonly room: string,
        public readonly addMessage: (msg: Message | SystemMessage,
                                     fresh: boolean) => void,
        public readonly changeVote: (vote: VoteChange) => void,
    ) { }

    /**
     * Start receiving messages from the room.
     */
    public async connect() {
        // Load history.
        const res = await fetch(`/${this.room}/history`);
        const data = await res.json();
        for (const msg of data) {
            this.addMessage(msg, false);
        }

        // Listen for new events.
        const source = new EventSource(`/${this.room}/chat`);
        source.addEventListener('open', (event) => {
            console.log("open", event);
        });
        source.addEventListener('error', (event) => {
            console.log("error", event);
        });
        source.addEventListener('message', (event) => {
            console.log("received message", event);
            this.addMessage(JSON.parse(event.data), true);
        });
        source.addEventListener('vote', (event) => {
            console.log("received vote", event);
            this.changeVote(JSON.parse((event as MessageEvent).data));
        });
    }

    /**
     * Try to resume an old session for this room, returning the current
     * username if the existing session is still valid.
     */
    async resume_session(): Promise<string | null> {
        const old_sess = localStorage.getItem(`session:${this.room}`);
        if (!old_sess) {
            return null;
        }

        let res;
        try {
            res = await fetch(`/${this.room}/session`, {
                headers: { 'Session': old_sess },
            });
        } catch (e) {
            return null;
        }
        const sess_data = await res.json();

        this.session = old_sess;
        return sess_data.user;
    }

    /**
     * Establish a brand-new session with a given username.
     */
    async new_session(user: string) {
        const res = await fetch(`/${this.room}/session`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({user: user}),
        });
        const new_sess = await res.text();

        localStorage.setItem(`session:${this.room}`, new_sess);
        this.session = new_sess;
    }

    /**
     * Resume or start a session in this room, which allows sending messages.
     */
    public async open_session(user: string) {
        // Try reusing an old session, if any exists.
        const old_user = await this.resume_session();
        if (old_user) {
            console.log(`resumed session ${this.session} as ${old_user}`);
        } else {
            await this.new_session(user);
            console.log(`started session ${this.session} as ${user}`);
        }
    }

    /**
     * Send a message. There must be an open session.
     */
    public async send(msg: string) {
        await fetch(`/${this.room}/message`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                'Session': this.session!,
            },
            body: msg,
        });
    }

    /**
     * Change the current user. There must be an open session.
     */
    public async setUser(user: string) {
        await fetch(`/${this.room}/session`, {
            method: 'PUT',
            headers: {
                'Content-Type': 'application/json',
                'Session': this.session!,
            },
            body: JSON.stringify({user}),
        });

        this.addMessage({
            body: `you are now known as ${user}`,
            system: true,
        }, true);
    }

    /**
     * Vote for a message. There must be an open session.
     */
    public async vote(msgId: string, vote: boolean) {
        await fetch(`/${this.room}/message/${msgId}/vote`, {
            method: 'POST',
            headers: { 'Session': this.session! },
            body: vote ? "1" : "0",
        });
    }

    /**
     * Get the IDs of messages the current session has voted for.
     */
    public async get_votes(): Promise<string[]> {
        const res = await fetch(`/${this.room}/votes`, {
            headers: { 'Session': this.session! }
        });
        return await res.json();
    }
}

interface ViewElements {
    readonly out: HTMLElement;
    readonly outContainer: HTMLElement;
    readonly form: HTMLFormElement;
    readonly msg: HTMLInputElement;

    readonly msgTmpl: Element;
    readonly sysTmpl: Element;
}

class View {
    client: Client | undefined;

    constructor(
        public readonly els: ViewElements,
    ) {
        this.els.form.addEventListener('submit', async (event) => {
            event.preventDefault();
            const text = this.els.msg.value;

            if (text.startsWith(USERNAME_CMD)) {
                // Update username.
                const newname = text.split(' ')[1];
                await this.client!.setUser(newname);
                this.setUser(newname);
            } else {
                // Fire and forget; no need to await.
                this.client!.send(text);
            }

            this.els.form.reset();
        });
    }

    setUser(user: string) {
        localStorage.setItem('username', user);
    }

    getUser(): string {
        return localStorage.getItem('username') || DEFAULT_USERNAME;
    }

    addMessage(msg: Message | SystemMessage, fresh: boolean) {
        let line: HTMLElement;
        if ("system" in msg) {
            line = this.els.sysTmpl.cloneNode(true) as HTMLElement;
        } else {
            const l = this.els.msgTmpl.cloneNode(true) as HTMLElement;
            l.dataset['id'] = msg.id;
            l.dataset['votes'] = msg.votes.toString();
            l.querySelector('.user')!.textContent = `${msg.user}:`;
            l.querySelector('.vote')!.addEventListener('click',
                this.handleVote.bind(this));
            l.querySelector('.vote .count')!.textContent =
                msg.votes ? msg.votes.toString() : "";
            line = l;
        }
        line.querySelector('.body')!.textContent = msg.body;

        if (fresh) {
            line.classList.add("fresh");
            setTimeout(() => line.classList.add("done"), 0);
        }

        this.els.out.appendChild(line);
        this.els.outContainer.scrollTop = 0;
    }

    async handleVote(event: Event) {
        const msg = (event.target as Element).parentElement!;
        const id = msg.dataset['id']!;
        const voted = msg.classList.contains('voted');

        console.log(`voting ${!voted} for ${id}`);
        await this.client!.vote(id, !voted);

        if (voted) {
            msg.classList.remove("voted");
        } else {
            msg.classList.add("voted");
        }
    }

    getMsgEl(id: string) {
        return this.els.out.querySelector<HTMLElement>(
            `[data-id="${id}"]`
        )!;
    }

    changeVote(vote: VoteChange) {
        const msg = this.getMsgEl(vote.message);
        const votes = parseInt(msg.dataset['votes']!) + vote.delta;
        msg.dataset['votes'] = votes.toString();
        msg.querySelector('.vote .count')!.textContent =
            votes ? votes.toString() : "";
    }

    showVotes(votes: string[]) {
        for (const voteId of votes) {
            const voteMsg = this.getMsgEl(voteId);
            voteMsg.classList.add('voted');
        }
    }
}

function loadTemplate(id: string): Element {
    const tmpl = document.getElementById(id)! as HTMLTemplateElement;
    return tmpl.content.firstElementChild!;
}

window.addEventListener('DOMContentLoaded', async (event) => {
    const view = new View({
        out: document.getElementById("messages")!,
        outContainer: document.getElementById("output")!,
        form: document.getElementById("send")! as HTMLFormElement,
        msg: document.getElementById("sendMessage")! as HTMLInputElement,

        msgTmpl: loadTemplate("tmplMessage"),
        sysTmpl: loadTemplate("tmplSysMessage"),
    });

    const client = new Client(BAND_ROOM_ID,
        view.addMessage.bind(view), view.changeVote.bind(view));
    view.client = client;

    const connect_fut = client.connect();
    const session_fut = client.open_session(view.getUser());
    await connect_fut;
    await session_fut;

    const votes = await client.get_votes();
    console.log(`loaded ${votes.length} previous votes`);
    view.showVotes(votes);
});
