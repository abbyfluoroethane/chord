# Showcase data

The people, spaces and conversations that the desktop preview and the Android showcase
screenshots use. Both apps follow this file, so a screenshot of either app tells the same story.
The art in `avatars/`, `spaces/` and `photos/` is drawn by `make_art.py` (run it with
`python3 make_art.py`), except `photos/wannacry.jpg`, the thumbnail of the official YouTube video
(see "Rights" below).

Tone: a friend group talking the way people really do in a group chat. Lowercase, short, a bit
messy. No full names, no jokes written for the reader, no statements of fact that we did not
check. Most messages have no reactions.

## You

nina, `nina@chat.foid.space`, avatar `maya.png`, available, no status.

## People

Display names are nicknames, lowercase, the way people set them.

| Name | Address | Avatar | Presence | Status |
| --- | --- | --- | --- | --- |
| marco | marco@chat.foid.space | theo.png | available | |
| jess | jess@chat.foid.space | priya.png | available | |
| theo | theo@chat.foid.space | luis.png | away | "at work" |
| sam | sam@chat.foid.space | none (initials) | available | |
| kai | kai@xmpp.example.net | noor.png | available | |
| lena | lena@chat.foid.space | ada.png | do not disturb | |
| rory | rory@chat.foid.space | kenji.png | offline | |
| ben | ben@chat.foid.space | none (initials) | offline | |

kai is on another server, which shows that Chord talks to any XMPP server.

## Spaces (rail order)

1. **basement** (`basement.png`): the friend group's server. Channels: `general`, `music`
   (topic "post songs"), `pics`, `games`, `memes` (muted). No categories.
2. **night owls** (`night-owls.png`): a gaming group. `lfg`, `clips`.
3. **film club** (`darkroom.png`): `show-and-tell`, `general`.
4. **climbing** (`crag.png`): `sessions` (topic "tue / thu / sat").
5. **Sandbox** (no icon): every edge case the preview needs for development: failed send,
   retracted message, a sender who is not a contact, audio, video, file, spoiler, Discord-style
   timestamps, `/me`, a long code block, xmpp: links. Phrase these plainly too. It sits last.

## The hero conversation: basement / #music

The main screenshots show this channel. Times are today, in the evening.

1. **nina** (you) 21:04: new ninajirachi is out
2. **nina** 21:04: https://www.youtube.com/watch?v=Ob_EDY9Eiis
   Link preview: site "YouTube", title "Ninajirachi & Porter Robinson - WannaCry [Official Visualiser]",
   description "Ninajirachi", image `photos/wannacry.jpg` (1280x720). Reactions: 🔥 4 (one is yours).
3. **marco** 21:05: WITH PORTER??
4. **nina** 21:05: with porter
5. **jess** 21:07: ok this goes so hard
6. **marco** 21:09: the drop is insane
7. **jess** 21:10, reply to marco (6): is this the one from coachella
8. **nina** 21:10: yeah he came out for it in april
9. **theo** 21:14: adding it to the car playlist
10. **kai** 21:15: is she touring this year
11. Typing: marco is typing.

The unread divider ("New") sits before message 9. Message 7 is a reply.

## Other channels (short)

- **basement / #general**: theo: "who left a hoodie at mine". jess: "grey one?". theo: "yeah". jess: "thats sam's". sam: "i was wondering where that went".
- **basement / #pics**: theo shares `harbor-sunset.png`: "sunset from the ferry". nina: "wait where is this". theo: "coming back from the island". jess: "jealous".
- **night owls / #lfg**: marco: "lobby in 10?". rory: "give me 15 im eating". marco: "ok 15". lena: "im in".
- **film club / #show-and-tell**: lena shares `hills.png`: "first roll from the new camera". kai: "the haze in the back 👌". lena: "it was so foggy that morning".
- **climbing / #sessions**: sam: "thursday?". jess: "cant, work". sam: "saturday then". jess: "saturday works".

## Home (direct and group chats)

- **jess** (DM, unread 1): jess: "are you going to the show on the 18th". nina: "if i can get off work". jess: "ill grab 2 just in case".
- **sunday dinner** (group chat: nina, marco, theo, jess, unread 2): marco: "still on for sunday?". theo: "yes my place". theo: "bring chairs if you have them".
- **marco**, **kai**, **lena**: older DMs, nothing unread.

## Contacts

Everyone in "People" is a contact. One incoming request from mika (`mika@xmpp.example.net`),
one outgoing request to ines (`ines@xmpp.example.net`), one blocked address (`promo@spam.example`).

## Members of basement

Owner: theo. Admins: nina. Members: marco, jess, sam, kai, lena, rory, ben.

## Rights

`photos/wannacry.jpg` is the thumbnail of the official visualiser on Ninajirachi's YouTube
channel. It shows the artists' artwork. It is fine for internal screenshots; before a public
promotional use, decide whether to keep it or swap in neutral art.
