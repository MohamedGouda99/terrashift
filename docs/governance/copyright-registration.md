# Copyright registration — practical guide

**Status:** action items for the copyright holder. Not legal advice.

A license without registration is harder to enforce. This guide walks
through registering Terrashift's copyright in the two jurisdictions
that matter most for a project hosted on GitHub:

1. **Egypt** (where the author resides) — primary domicile registration.
2. **United States** (where most infringers and US-based companies are
   reachable) — required for statutory damages and attorney fees in
   US federal court.

Register both. The cost is modest; the leverage difference in
litigation is enormous.

---

## Why register

Copyright exists automatically the moment you fix a creative work in a
tangible medium (writing the code is enough). What registration adds:

| Without registration | With registration |
|---|---|
| Can sue, but only for **actual damages** (often hard to prove) | Can claim **statutory damages** ($750–$30,000 per work, up to $150,000 if willful) |
| Cannot recover **attorney fees** | Can recover attorney fees — makes lawyers willing to take the case on contingency |
| Burden of proof is high; you must prove authorship and date | Registration creates a **legal presumption** of validity and ownership |
| US courts won't accept the case at all unless you register first | You can file suit in any US district court |

The same pattern holds in most jurisdictions: registration buys
*evidence* and *remedies*.

---

## Egypt — Ministry of Culture

The Egyptian Authority for the Protection of Intellectual Property
Rights handles copyright registration under **Law No. 82 of 2002 on
the Protection of Intellectual Property Rights** (Book Three —
Copyright and Neighbouring Rights).

### Steps

1. **Prepare a copy of the work:**
   - Print or burn-to-CD a snapshot of the repository at a specific
     commit.
   - Include: source code (`cli/`, `tui/`, `libs/`), `LICENSE`,
     `README.md`, `ATTRIBUTIONS.md`, the architecture documents.
   - Note the commit SHA and date on the cover page.

2. **Author identification:**
   - Egyptian National ID
   - Address
   - Authorship statement signed by the author

3. **Submission venue:**
   - The Egyptian Patent Office / Information Technology Industry
     Development Agency (ITIDA) handles software copyright deposits.
   - Address: 6 Adly Street, Downtown Cairo (verify current address
     before going).
   - Some submissions can be done online via ITIDA's portal.

4. **Fee:** approximately EGP 500–1500 for software registration
   (verify current rates — they change).

5. **Timeline:** typically 30–90 days for a registration certificate.

6. **Output:** an official certificate naming you as author, with the
   work's title, deposit date, and a registration number. Keep this
   certificate; it's your evidence in any future dispute.

### What this gets you

- Egyptian courts recognise the registration as prima facie evidence
  of ownership.
- The deposited copy at the registry is a tamper-evident record of
  what existed on the deposit date.
- Treaty obligations (Berne Convention — Egypt is a signatory) mean
  the registration also gives you standing in courts of every other
  Berne signatory (170+ countries) to sue for infringement, though
  remedies vary by jurisdiction.

---

## United States — Copyright Office

The US Copyright Office is at copyright.gov. Registration is a
straightforward online process.

### Steps

1. **Choose the application type:**
   - For software, use **Form TX** (literary works — software is
     classified as a literary work in the US).
   - File via the eCO (Electronic Copyright Office) portal at
     https://eco.copyright.gov.

2. **Required information:**
   - Title: "Terrashift"
   - Author name: Mohamed Gouda
   - Year of completion: 2026 (the year the bulk of the code was
     written; you can update with later versions)
   - Author's nationality: Egyptian
   - Whether the work was created as a work-for-hire: No (assuming
     this is your personal project)
   - Publication status: Published (because it's on GitHub) or
     Unpublished (if you treat the GitHub publication as not
     constituting "publication" — debatable; consult a lawyer)

3. **Deposit copy:**
   - For published software: deposit the first 25 and last 25 pages
     of source code. The Copyright Office accepts redactions if some
     code contains trade secrets.
   - For Terrashift: the first 25 pages of `cli/src/main.rs` +
     `cli/src/commands/migrate.rs` and the last 25 pages of, say,
     `libs/engine/src/generator/templates.rs` is fine.
   - Deposit can be uploaded as PDF.

4. **Fee:**
   - Single author, one work, online: **\$45 USD**
   - Standard application: **\$65 USD**
   - For Terrashift, single-author, the \$45 tier applies.

5. **Timeline:** 3–6 months for a certificate, but the registration
   is **effective from the day you file**, not the day you receive
   the certificate.

6. **Output:** a certificate of registration with a "TX" registration
   number. Keep it.

### Critical timing note

You can register at any time, but for **statutory damages and
attorney fees in a US infringement suit**, you must register within
**three months of first publication** — OR before the infringement
began. So:

- If you register **now** (within three months of publishing on
  GitHub), every future infringement is covered for statutory
  damages.
- If you register **after** discovering an infringement, you can
  still sue, but you're limited to actual damages on that specific
  infringement. Future infringements (after registration) get
  statutory damages.

**Action item:** register within the first three months of putting
the project on GitHub. Don't wait.

---

## What to register

Register the **whole repository as a single work**, not file-by-file.
The US Copyright Office accepts registration of an entire program,
even when it's many files.

You can update the registration with new versions later (form CA for
corrections, or just file a fresh registration for a major version).
For a fast-moving codebase, registering once a quarter or twice a
year is reasonable.

---

## After registration

1. **Add the registration number to the LICENSE preamble** (optional
   but useful):
   ```
   Copyright © 2026 Mohamed Gouda. All Rights Reserved.
   US Copyright Reg: TX 1-234-567 (filed 2026-XX-XX)
   Egyptian Copyright Reg: [number] (filed 2026-XX-XX)
   ```

2. **Keep the certificates safe.** Original certificate + 2 photo
   copies. One copy goes in a fireproof box; the other goes off-site.

3. **Watch for infringement.** GitHub's content code search, Google
   Code Search alerts, and tools like Sourcegraph can surface forks
   or copies of your distinctive identifier strings (e.g., `terrashift`,
   `Terrashift Source-Available License`, your custom error messages).

4. **First response to discovered infringement:** a polite DMCA
   takedown notice to the host (GitHub, Bitbucket, GitLab, the
   infringer's web host). DMCA takedowns are free, fast, and often
   sufficient. Keep records of every notice you send and every
   response.

---

## What this does NOT cover

- **Trademark** — "Terrashift" as a name. Trademark is separate from
  copyright. If you want to prevent someone else from naming their
  product "Terrashift", file a trademark application separately. In
  the US: USPTO TEAS Plus, ~\$250–350 per class. In Egypt: TMRA, fees
  vary.
- **Patents** — methods or algorithms in the code. Software patents
  are a much bigger and more expensive undertaking; usually overkill
  for a personal project.
- **Trade secrets** — anything you keep private and don't ship. By
  publishing on GitHub, you've waived trade secret protection on the
  published parts. Anything you keep private (e.g., commercial
  variants, customer data) can still be protected as a trade secret
  under separate doctrine.

---

## A reasonable order of operations

1. **Today / this week:** read the new LICENSE, decide if it captures
   your intent. Talk to a lawyer if the stakes are real.
2. **Within 3 months of GitHub publication:** file US Copyright Office
   registration ($45 online, ~10 minutes of form-filling). This is
   the single highest-leverage protective step you can take.
3. **Within the next 3-6 months:** file Egyptian copyright registration
   for primary-domicile evidence.
4. **Ongoing:** monitor GitHub and Google for unauthorised copies. Send
   DMCA takedowns when you find them. Document everything.
5. **If a serious infringement happens:** consult a lawyer. Your
   registrations + commit history + LICENSE file are the evidence
   they'll work with.

---

**This guide is informational only and does not constitute legal advice.
Consult a qualified attorney before relying on it for litigation
decisions.**
