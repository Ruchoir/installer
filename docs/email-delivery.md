# Email delivery

Moved from `docs/email-delivery.md` of the former monolith (`Ruchoir/ruchoir`) on 2026-11-11, and brought up to date with this repository.

Two services of an instance send mail:

- **Propolis**: the address confirmation (a link and a six-digit code), the password reset, an
  invitation into an organisation addressed to someone's mailbox, the reminders to a guest before
  their access ends, and the digest of what stays unread, which the hub asks it to send.
- **The calendar**, when installed: the invitations to people invited by address.

The hub, the drive and the chat send none themselves.

A development instance (`ruchoir-install dev`) sends every mail to Mailpit, read at
`http://localhost:8025`. A production instance has **no relay** until you give it one: each service
then writes its mails to its log, links and codes included (`docker compose logs propolis`, in the
instance's directory), and nothing leaves. This page is about the day you want them to arrive.

## What works without a relay

- **Joining.** An invitation addressed to someone's address proves that address: the account it
  creates is active at once, with no confirmation. An account created without one waits for a
  confirmation that only reaches Propolis's log.
- **Getting back in.** Someone who has kept their recovery codes resets their own password with one,
  from the "forgot password" page.

Everything else that is mailed (the guests' reminders, the digest, the calendar's invitations by
address) is simply not received.

## Which path is yours

| | A relay | Your own sending server |
|---|---|---|
| What it is | You hand each message to a provider's outbound server, authenticated with a username and a password | Your host talks to the recipients' mail servers itself |
| What it needs | An account and a few settings | A fixed address, control of the domain's DNS, outbound port 25, and a reputation you build over time |
| Reasonable when | Almost always, and always at first | The instance already runs on infrastructure you administer, with a static address and a matching reverse DNS record |

**If the instance runs on a home connection, take a relay.** A residential address cannot deliver
mail: consumer ISPs block outbound port 25, the address sits in blocklists made for consumer ranges,
and its reverse DNS name belongs to the ISP. Messages sent from there are not bounced so much as
silently dropped.

A relay is not a surrender of sovereignty either: it is one European provider, swapped for your own
server the day you have the infrastructure to run one, by changing the same few settings.

The installer ships no mail server, on purpose. A mail server is a long-lived, internet-facing
service with its own security surface and its own operational discipline, and being accepted by the
receiving side has almost nothing to do with the sending software. If you run your own, run it as
your own: [Stalwart](https://stalw.art) (Rust, AGPL) and [Maddy](https://maddy.email) (Go, GPL) both
do outbound-only sending well.

## Path 1: a relay

Put the relay in `compose.override.yaml`, in the instance's directory, which `init` never touches,
and its password in `.env`, which `init` keeps, as `SMTP_PASSWORD=...`:

```yaml
services:
  propolis:
    environment:
      PROPOLIS_SMTP_HOST: smtp.example.eu
      PROPOLIS_SMTP_PORT: "587"
      PROPOLIS_SMTP_USERNAME: ruchoir@example.org
      PROPOLIS_SMTP_PASSWORD: ${SMTP_PASSWORD}
      PROPOLIS_SMTP_FROM: Ruchoir <no-reply@example.org>
  # With the calendar only: Compose refuses a service the instance does not have.
  calendar:
    environment:
      CALENDAR_SMTP_HOST: smtp.example.eu
      CALENDAR_SMTP_PORT: "587"
      CALENDAR_SMTP_USERNAME: ruchoir@example.org
      CALENDAR_SMTP_PASSWORD: ${SMTP_PASSWORD}
      CALENDAR_SMTP_FROM: Ruchoir <no-reply@example.org>
```

Then `ruchoir-install up --dir /srv/ruchoir`: Compose recreates the services whose settings changed.
`docker compose restart` would not: a container keeps the environment it was created with.

The links in the messages are built from each service's public address, which the installer sets
from the domain (`https://id.<domain>`), so there is nothing to add for them.

**Both TLS dialects, and the port picks one.** Port 587 opens in clear and upgrades with STARTTLS;
port 465 is encrypted from the first byte. Get it wrong and you will not see an error, you will see
nothing: speaking STARTTLS to a port expecting implicit TLS (or the reverse) does not fall back, it
hangs until the connection times out. If mail neither arrives nor fails, check the port first.
`PROPOLIS_SMTP_TLS=none` speaks in clear, and is refused unless the relay is this machine or an
internal name with no dot in it: it is for a mail catcher, never for real mail.

**The default looks configured.** Without `PROPOLIS_SMTP_FROM`, mails leave from
`Ruchoir <no-reply@localhost>`, which a relay refuses. The address must belong to a domain the relay
has verified.

European providers, for the same reason every other dependency here is European:

| Provider | Where | Note |
|---|---|---|
| Scaleway Transactional Email | France | A free monthly allowance, then a price per thousand. No mailbox needed |
| IONOS | Germany / France | A relay comes with the domain, but its SMTP authenticates with a mailbox login, so it costs a paid mailbox even if nobody reads it |
| Infomaniak | Switzerland | Same shape as Scaleway, Swiss hosting |

**A domain with no mailbox is not a dead end.** Sending and receiving are separate: a transactional
relay signs and sends for a domain it has verified, and nobody needs a mailbox behind the address.

### Scaleway, concretely

There is no SMTP password. The username is the identifier the Transactional Email page shows (the
project id), and the password is the **secret key** of a Scaleway API key, displayed once at creation.
Generate that key on a dedicated IAM application holding only the Transactional Email permission: a
default key can create servers, and the one that lives in a `.env` on a server should be able to do
exactly one thing.

```yaml
      PROPOLIS_SMTP_HOST: smtp.tem.scaleway.com
      PROPOLIS_SMTP_PORT: "587"
      PROPOLIS_SMTP_USERNAME: <the identifier the console shows>
      PROPOLIS_SMTP_PASSWORD: ${SMTP_PASSWORD}
      PROPOLIS_SMTP_FROM: Ruchoir <no-reply@your-verified-domain>
```

The domain has to be verified in Transactional Email first, with the SPF and DKIM records published
in its DNS zone.

Whichever relay you pick, the sending domain has to authorise it: the provider gives you an SPF entry
and DKIM records to publish. Publish them. A relay with no authorisation from your domain is a
stranger claiming to be you, and it is treated as one.

## Path 2: your own sending server

Everything below has to be true at once. Any one of them missing is enough to have mail quietly
discarded rather than rejected.

1. **A fixed public address**, and outbound port 25 open from it. Most residential and many
   low-cost VPS connections block it; some providers open it on request.
2. **A reverse DNS (PTR) record** for that address, resolving to the name the server announces in its
   HELO. Mismatched forward and reverse names is the most common reason a message is refused.
3. **SPF**: a TXT record on the sending domain listing that address as allowed to send for it.
4. **DKIM**: a signing key, published as a TXT record, with the server signing every outgoing
   message.
5. **DMARC**: a policy record saying what a receiver should do with a message failing the two above.
   Start at `p=none` and read the reports before tightening it.
6. **Patience.** A brand-new address has no reputation. Send little, send to people who expect it,
   and expect the first weeks to be worse than the months after.

Point `PROPOLIS_SMTP_HOST` (and `CALENDAR_SMTP_HOST`) at that server as at any relay.

## Testing it before you rely on it

Propolis's log is the first answer: `email not sent (no SMTP relay configured)` means nothing is
leaving. Once a relay is set and the services recreated, that line is gone. Then prove it end to
end:

1. Ask for a password reset for an address you can read, and check that the message arrives at all.
2. Open its full headers and look for `spf=pass`, `dkim=pass` and `dmarc=pass`. A message that
   arrives with any of them failing is one filter rule away from not arriving.
3. Check the link in the body points at `https://id.<domain>`.
4. Send one to a mailbox on a large consumer provider, not only to your own domain.

[mail-tester.com](https://www.mail-tester.com) (France) scores all of the above from a single message.
It is a third-party service: what you send it, it sees, so send it a test message and not a real one.

The recovery codes stay useful with a relay: a relay that works today can be down on the morning
someone is locked out.
