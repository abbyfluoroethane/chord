# Showcase data

The people, spaces and conversations that the desktop preview and the Android showcase
screenshots use. Both apps follow this file, so a screenshot of either app tells the same story.
The art in `avatars/`, `spaces/` and `photos/` is drawn by `make_art.py` (run it with
`python3 make_art.py`). It is abstract on purpose: no photos of real people.

Tone: people talk like people in a small, friendly community. Short messages, plain words, a few
typos are fine, no jokes written for the reader, no marketing lines, no emoji walls. Nobody
explains the app. Most messages have no reactions.

## You

Maya Okafor, `maya@chat.foid.space`, avatar `maya.png`, available, status "Playtest week".

## People

| Name | Address | Avatar | Presence | Status |
| --- | --- | --- | --- | --- |
| Priya Raman | priya@chat.foid.space | priya.png | available | |
| Theo Lindqvist | theo@chat.foid.space | theo.png | available | "Profiling saves" |
| Jules Moreau | jules@chat.foid.space | jules.png | away | |
| Kenji Ito | kenji@chat.foid.space | kenji.png | available | |
| Ada Novak | ada@chat.foid.space | ada.png | do not disturb | "Writing" |
| Noor Haddad | noor@xmpp.example.net | noor.png | available | |
| Luis Ortega | luis@chat.foid.space | luis.png | offline | |
| Sam Delgado | sam@chat.foid.space | none (initials) | away | |
| Wren Callahan | wren@chat.foid.space | none (initials) | offline | |

Noor is on another server, which shows that Chord talks to any XMPP server.

## Spaces (rail order)

1. **Lantern Works** (`lantern-works.png`): a small indie game studio and its playtesters.
   Categories and channels:
   - (none): `announcements` (topic "Builds and dates. Read only."), `general`
   - Playtest: `playtest` (topic "Build 0.14.2 is live. Bugs go in #bugs."), `bugs`, `ideas`
   - Studio: `art`, `audio`, `off-topic` (muted)
2. **Darkroom** (`darkroom.png`): a film photography club. `general`, `show-and-tell`, `developing`, `gear-swap`.
3. **Crag Club** (`crag.png`): a bouldering group. `general`, `sessions` (topic "Tue and Thu, 18:30").
4. **Slow Readers** (`slow-readers.png`): a book club. `general`, `this-month` (topic "The Left Hand of Darkness, ch. 1 to 8").
5. **Sandbox** (no icon): every edge case the preview needs for development: failed send,
   retracted message, a sender who is not a contact, audio, video, file, spoiler, Discord-style
   timestamps, `/me`, a long code block, xmpp: links. Phrase these plainly too. It sits last.

## Home (direct and group chats)

- Theo (unread 1): about lunch, see below.
- Priya: quiet, no unread.
- "Lantern core" group chat (Maya, Priya, Theo, Kenji), unread 2.
- Noor, Ada, Jules: older chats.

## The hero conversation: Lantern Works / #playtest

This is the channel that the main screenshots show. Times are today, in the morning.

1. **Priya** 09:12: Build 0.14.2 is up on the playtest branch. New save system, controller remapping, and the lighthouse level is finally in.
2. **Priya** 09:12: Please break it 🙏
3. **Theo** 09:20: downloading
4. **Jules** 09:47: Played the lighthouse twice. The fog at the top looks great, but I lost the rope prompt both times. It spawns behind the camera. *(attachment: `lighthouse-fog.png`, 1280x720)*
5. **Priya** 09:51, reply to Jules (4): Good catch. The prompt is placed in world space, I'll pin it to the screen edge when it's off camera.
6. **Kenji** 09:53: something like this?
   ```gdscript
   if not camera.is_position_in_frustum(prompt.global_position):
       prompt.pin_to_edge(camera)
   ```
7. **Priya** 09:54: yes, almost exactly. Want to open a PR? — reactions: 👍 3 (one is yours)
8. **Kenji** 09:54: on it
9. **Maya** (you) 10:02: @Theo can you check that old saves still load? I don't want to ship 0.14 if anyone loses progress.
10. **Theo** 10:05: On it. Three saves from 0.13 so far, all load fine. The cloud one takes about 4 s, I'll profile it.
11. **Ada** 10:31: Draft of the patch notes: https://lanternworks.example/notes/0-14 *(link preview: site "Lantern Works", title "Patch notes 0.14: The Lighthouse", description "Controller remapping, a new save system, and a lighthouse that took us far too long.", image `patch-notes.png`)*
12. **Sam** 10:40: Read it. I'd lead with remapping, that's what people asked for most in the survey. — reactions: ❤️ 2
13. **Ada** 10:42: fair, swapping them
14. Typing: Kenji is typing.

The unread divider ("New") sits before message 11. Theo's message 10 mentions nobody; message 9
mentions Theo.

## Other channels (short, 3 to 8 messages each)

- **Lantern Works / #announcements**: Priya: "0.14.2 is out on the playtest branch. Saves from 0.13 carry over." (2 days ago); Priya: "Playtest call is Friday at 17:00 UTC." (today 08:30).
- **Lantern Works / #bugs**: Jules: "Controller remap screen: pressing B twice exits without saving." Kenji: "Repro'd. Fix is in 0.14.3." Noor: "Same on keyboard with Esc."
- **Lantern Works / #art**: Luis shares `hills.png`: "Background pass for the valley level. Too green?" Ada: "A bit. Try pulling the far hills toward blue." Luis: "yeah that's better, will push tonight".
- **Darkroom / #show-and-tell**: Noor shares `harbor-sunset.png`: "Portra 400, pushed one stop. Harbor at the end of the day." Wren: "the color in the water 😮". Noor: "Lab scan, no edits." Jules: "Which lab?" Noor: "The one on 3rd, they're slow but careful."
- **Crag Club / #sessions**: Sam: "Thursday as usual?" Theo: "I'm in. My fingers are still recovering from Tuesday." Wren: "Bringing the new tape." Sam: "18:30 at the wall then."
- **Slow Readers / #this-month**: Ada: "Chapter 6 changed how I read the first five." Priya: "Same. I went back to the Ekumen report." Ada: "No spoilers past 8 please 🙂"
- **Home / Theo** (DM): Theo: "lunch after the playtest call?" Maya: "Yes. Ramen place?" Theo: "perfect" (unread 1: Theo's last).
- **Home / Lantern core** (group): Kenji: "PR is up for the rope prompt." Priya: "Reviewing after standup."

## Contacts

Everyone in "People" is a contact. One incoming request from Mika Sato (`mika@xmpp.example.net`),
one outgoing request to Ines Duarte (`ines@xmpp.example.net`), and one blocked address
(`promo@spam.example`).

## Members of Lantern Works

Owner: Priya. Admins: Maya, Theo. Members: Kenji, Ada, Jules, Sam, Noor, Luis, Wren.
