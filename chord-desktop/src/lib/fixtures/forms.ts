// Sample forms for the browser preview, where there is no bridge. They show each field type.
import type { CommandItem, CommandStep, DataForm, RegistrationForm } from '$lib/chord/types';

const f = (over: Partial<DataForm['fields'][number]>): DataForm['fields'][number] => ({
  var: null,
  kind: 'text-single',
  label: null,
  desc: null,
  required: false,
  values: [],
  options: [],
  media: [],
  ...over
});

/** A room owner form like the one of ejabberd, shortened. */
export function sampleRoomForm(): DataForm {
  return {
    kind: 'form',
    title: 'Configuration of the room',
    instructions: 'Change the options of this room, then save.',
    fields: [
      f({ var: 'FORM_TYPE', kind: 'hidden', values: ['http://jabber.org/protocol/muc#roomconfig'] }),
      f({ var: 'muc#roomconfig_roomname', label: 'Room title', values: ['playtest'] }),
      f({ var: 'muc#roomconfig_roomdesc', label: 'Room description', kind: 'text-multi' }),
      f({ var: 'muc#roomconfig_publicroom', kind: 'boolean', label: 'Show in the room list', values: ['1'] }),
      f({ var: 'muc#roomconfig_membersonly', kind: 'boolean', label: 'Only members can join', values: ['0'] }),
      f({ var: 'muc#roomconfig_persistentroom', kind: 'boolean', label: 'Make room persistent', values: ['1'] }),
      f({ var: 'muc#roomconfig_roomsecret', kind: 'text-private', label: 'Password' }),
      f({
        var: 'muc#roomconfig_maxusers',
        kind: 'list-single',
        label: 'Maximum number of occupants',
        values: ['50'],
        options: ['10', '20', '50', '100'].map((v) => ({ label: v, value: v }))
      }),
      f({
        var: 'muc#roomconfig_whois',
        kind: 'list-single',
        label: 'Show real addresses to',
        values: ['moderators'],
        options: [
          { label: 'Moderators only', value: 'moderators' },
          { label: 'Anyone', value: 'anyone' }
        ]
      }),
      f({
        var: 'muc#roomconfig_presencebroadcast',
        kind: 'list-multi',
        label: 'Roles that get presence',
        values: ['moderator', 'participant'],
        options: [
          { label: 'Moderator', value: 'moderator' },
          { label: 'Participant', value: 'participant' },
          { label: 'Visitor', value: 'visitor' }
        ]
      }),
      f({ var: 'muc#roomconfig_moderatedroom', kind: 'boolean', label: 'Make room moderated', values: ['0'] }),
      f({ var: 'muc#roomconfig_allowinvites', kind: 'boolean', label: 'Allow members to send invites', values: ['1'] })
    ]
  };
}

export const sampleCommands: CommandItem[] = [
  { jid: 'chat.foid.space', node: 'http://jabber.org/protocol/admin#add-user', name: 'Add a user' },
  { jid: 'chat.foid.space', node: 'ping', name: 'Ping' }
];

/** The first step of a command with two steps, and the last one. */
export function sampleSteps(): CommandStep[] {
  return [
    {
      node: 'http://jabber.org/protocol/admin#add-user',
      sessionId: 'sample',
      status: 'executing',
      actions: ['next'],
      defaultAction: 'next',
      notes: [],
      form: {
        kind: 'form',
        title: 'Add a user',
        instructions: 'Fill out this form to add a user.',
        fields: [
          f({ var: 'accountjid', kind: 'jid-single', label: 'The Jabber ID for the account', required: true }),
          f({ var: 'password', kind: 'text-private', label: 'Password', required: true })
        ]
      }
    },
    {
      node: 'http://jabber.org/protocol/admin#add-user',
      sessionId: 'sample',
      status: 'completed',
      actions: [],
      defaultAction: null,
      notes: [{ kind: 'info', text: 'Operation finished successfully' }],
      form: null
    }
  ];
}

export const sampleRegistration: RegistrationForm = {
  instructions: 'Choose a username and password to register with this server',
  form: null,
  fields: ['username', 'password'],
  oob: null,
  registered: false
};

/** The people of a room with one affiliation, for the preview. */
export function sampleAffiliations(affiliation: string): [string, string | null][] {
  if (affiliation === 'outcast') {
    return [
      ['promo@spam.example', null]
    ];
  }
  return [
    ['priya@chat.foid.space', 'Priya Raman'],
    ['theo@chat.foid.space', 'Theo Lindqvist'],
    ['kenji@chat.foid.space', null]
  ];
}
