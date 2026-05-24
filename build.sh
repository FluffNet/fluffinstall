#!/bin/bash

rustc fluffinstall.rs \
  -C opt-level=3 \
  -C lto=fat \
  -C codegen-units=1 \
  -C panic=abort \
  -C strip=symbols

