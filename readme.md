## multisig

_simple anchor multisig with a UI_

### developing

```
anchor build
anchor test
```

### deploying

Make sure to update `declare_id!` to `~/program-id.json`'s key first.

```
solana program deploy -u devnet -k ~/key.json --program-id ~/program-id.json target/deploy/multisig.so
```

Create a multisig instance

```
import fs from 'fs';
import anchor, { web3 } from "@project-serum/anchor";

const wallet = new anchor.Wallet(web3.Keypair.fromSecretKey("..."));
const connection = new anchor.web3.Connection("https://api.mainnet-beta.solana.com", "recent");
anchor.setProvider(new anchor.Provider(connection, wallet));
const idl = JSON.parse(fs.readFileSync("./target/idl/multisig.json", "utf-8"));
const programId = new PublicKey("...");
const program = new anchor.Program(idl, programId);

const base = anchor.web3.Keypair.generate();
const [key, bump] = await PublicKey.findProgramAddress([Buffer.from("multisig"), base.publicKey.toBuffer()], programId);
const owners = [wallet.publicKey];

await program.rpc.createMultisig(owners, new BN(1), new BN(0), bump, {
  accounts: {
    signer: wallet.publicKey,
    base: base.publicKey,
    multisig: key,
    systemProgram: web3.SystemProgram.programId,
  },
});

console.log("multisig:", key.toString());
```

Transfer the upgrade authority to the multisig

```
solana program set-upgrade-authority PROGRAM-ADDRESS --new-upgrade-authority MULTISIG-ADDRESS
```

Now you should be able to deploy and use the [multisig-ui](https://github.com/kiasaki/multisig-ui.sol) for you deployed program

### license

MIT
