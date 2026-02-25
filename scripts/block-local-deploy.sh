#!/bin/bash
echo "======================================================================="
echo "❌ STOP! LOCAL DEPLOYMENTS ARE BLOCKED ❌"
echo "======================================================================="
echo "This repository uses automated CI/CD for deployments."
echo "Do not deploy manually from your local machine."
echo "Please commit and push your changes to the relevant branch."
echo "======================================================================="
exit 1
