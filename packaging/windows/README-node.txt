MBL Federated Storage Node for Windows (v1.0.0)
=================================================

Welcome to mbl-node! This binary allows you to turn any secondary PC, server, or storage
box into a federated storage node for your central MBL media server.

Quick Start
-----------
1. Pair with your MBL Server:
   Generate a 6-digit pairing code in the MBL Web UI (Admin -> Workers & Nodes -> Pair Storage Node).
   Then run:
       mbl-node.exe pair --server http://<YOUR_MBL_SERVER_IP>:8420 --code <CODE> --name "My-Storage-Node"

2. Start the Node daemon:
       mbl-node.exe run

   The node will index local storage directories and periodically sync delta catalogs
   with your primary MBL server.
