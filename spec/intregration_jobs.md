* test relay use *
Test where we have no NAT (on either client) and need to use a relay
Based on `bingle_api_send_message_to_id_localnet` and extracting common code, run the same test with stun servers s1 and s2 taking parameter broken_nat=true
Ensure that common code is pulled out into functions.

*test non-root relay use*
Generate a new test based on `bingle_api_send_message_to_id_localnet`

After starting relay1 and relay2 (and stun servers) wait for relay availability. Start a new non-root relay relay3. (Use relay id and passphrase:
3RLYTSRX54G5WOPPPV4FYWRV2QXKIC5WRPM54YKXGVLTAFGUEIG2QN4DMQ
horror stuff huge crunch green marriage parent soon hamster tonight miracle company fee cup hard media shiver emotion hybrid shiver main cube lemon about obvious
(fund this before use)

Similarly, start another new non-root relay relay4.
4RLY44PVAFKYGLAZC4FQFZGRPWZZUBPEX3OBCCROJQYJ5MEOETLQY5CJLE
airport there model more limb audit surprise black recipe eagle rely switch sphere debate report chapter pig hope fabric open transfer behind tent absorb deal

Create 2 nodes now with ids 3USE and 4USE which should use relay3 and relay4 using the partition rule in select_indices
3USEZJSATQKNSQIIEPBF2RZCVMNFPX4FKX7ZEXTHM5JVAWCGTQ7CKRSLNY
sphere because network adult sudden butter hotel taxi soul stove spare design forget announce post shoulder pretty smile jump pipe guitar speed enjoy abstract equa
4USEYYL2SRO4RZQHZ5C5FCN6U7HD5LDBIMTPOBEAI4E74WQ2Q5RUTZLR6E
extra shrimp behind outer three brass style inquiry input permit pass empower jump cement recycle unit door escape doll dinosaur rude special cloth about major

Send a message from 3USE to 4USE and ensure it succeeds. [NOTE: this will fail due to lack of ripple]

Ensure that common code is pulled out into functions. Fund all addresses before use
