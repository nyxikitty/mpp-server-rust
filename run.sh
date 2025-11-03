#!/bin/bash

# MPP Server Run Script

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}MPP Server - Rust Edition${NC}"
echo ""

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}Error: Rust is not installed${NC}"
    echo "Please install Rust from: https://rustup.rs/"
    exit 1
fi

# Check if client directory exists
if [ ! -d "client" ]; then
    echo -e "${YELLOW}Warning: client/ directory not found${NC}"
    echo "Please copy the client files from the original Node.js server"
    echo ""
fi

# Parse arguments
MODE="dev"
if [ "$1" = "release" ] || [ "$1" = "prod" ]; then
    MODE="release"
fi

if [ "$MODE" = "release" ]; then
    echo -e "${GREEN}Building in release mode...${NC}"
    cargo build --release
    echo ""
    echo -e "${GREEN}Starting server in production mode...${NC}"
    cargo run --release
else
    echo -e "${GREEN}Building in development mode...${NC}"
    echo ""
    echo -e "${GREEN}Starting server...${NC}"
    cargo run
fi