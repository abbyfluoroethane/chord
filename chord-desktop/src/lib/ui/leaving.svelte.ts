// The "Leave Chord?" question for a masked link whose text names another host than its
// target (BRIDGESECURITY-12). `ExternalLinkModal` shows it. Nothing opens until the user clicks.

class Leaving {
  /** The link that waits for the answer: its real target and the text that the message showed. */
  asking = $state<{ href: string; text: string } | null>(null);

  ask(href: string, text: string) {
    this.asking = { href, text };
  }

  dismiss() {
    this.asking = null;
  }
}

export const leaving = new Leaving();
