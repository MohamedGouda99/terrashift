# 10 security groups. 10 of 50 source resources.

resource "aws_security_group" "web_a" {
  name        = "web-a"
  description = "Allow HTTPS web tier A"
  vpc_id      = aws_vpc.main.id
}

resource "aws_security_group" "web_b" {
  name        = "web-b"
  description = "Allow HTTPS web tier B"
  vpc_id      = aws_vpc.main.id
}

resource "aws_security_group" "web_c" {
  name        = "web-c"
  description = "Allow HTTPS web tier C"
  vpc_id      = aws_vpc.main.id
}

resource "aws_security_group" "web_d" {
  name        = "web-d"
  description = "Allow HTTPS web tier D"
  vpc_id      = aws_vpc.main.id
}

resource "aws_security_group" "web_e" {
  name        = "web-e"
  description = "Allow HTTPS web tier E"
  vpc_id      = aws_vpc.main.id
}

resource "aws_security_group" "app_a" {
  name        = "app-a"
  description = "Allow HTTP app tier A"
  vpc_id      = aws_vpc.main.id
}

resource "aws_security_group" "app_b" {
  name        = "app-b"
  description = "Allow HTTP app tier B"
  vpc_id      = aws_vpc.main.id
}

resource "aws_security_group" "app_c" {
  name        = "app-c"
  description = "Allow HTTP app tier C"
  vpc_id      = aws_vpc.main.id
}

resource "aws_security_group" "db_a" {
  name        = "db-a"
  description = "Allow Postgres db tier A"
  vpc_id      = aws_vpc.main.id
}

resource "aws_security_group" "db_b" {
  name        = "db-b"
  description = "Allow Postgres db tier B"
  vpc_id      = aws_vpc.main.id
}
