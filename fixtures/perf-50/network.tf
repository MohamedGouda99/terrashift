# 1 VPC + 10 subnets — the network spine.
#
# 11 of the 50 source resources.

resource "aws_vpc" "main" {
  cidr_block = "10.0.0.0/16"

  tags = {
    Name = "perf-50-vpc"
  }
}

resource "aws_subnet" "public_a" {
  vpc_id     = aws_vpc.main.id
  cidr_block = "10.0.1.0/24"

  tags = {
    Name = "public-a"
    Tier = "public"
  }
}

resource "aws_subnet" "public_b" {
  vpc_id     = aws_vpc.main.id
  cidr_block = "10.0.2.0/24"

  tags = {
    Name = "public-b"
    Tier = "public"
  }
}

resource "aws_subnet" "public_c" {
  vpc_id     = aws_vpc.main.id
  cidr_block = "10.0.3.0/24"

  tags = {
    Name = "public-c"
    Tier = "public"
  }
}

resource "aws_subnet" "public_d" {
  vpc_id     = aws_vpc.main.id
  cidr_block = "10.0.4.0/24"

  tags = {
    Name = "public-d"
    Tier = "public"
  }
}

resource "aws_subnet" "public_e" {
  vpc_id     = aws_vpc.main.id
  cidr_block = "10.0.5.0/24"

  tags = {
    Name = "public-e"
    Tier = "public"
  }
}

resource "aws_subnet" "private_a" {
  vpc_id     = aws_vpc.main.id
  cidr_block = "10.0.10.0/24"

  tags = {
    Name = "private-a"
    Tier = "private"
  }
}

resource "aws_subnet" "private_b" {
  vpc_id     = aws_vpc.main.id
  cidr_block = "10.0.11.0/24"

  tags = {
    Name = "private-b"
    Tier = "private"
  }
}

resource "aws_subnet" "private_c" {
  vpc_id     = aws_vpc.main.id
  cidr_block = "10.0.12.0/24"

  tags = {
    Name = "private-c"
    Tier = "private"
  }
}

resource "aws_subnet" "private_d" {
  vpc_id     = aws_vpc.main.id
  cidr_block = "10.0.13.0/24"

  tags = {
    Name = "private-d"
    Tier = "private"
  }
}

resource "aws_subnet" "private_e" {
  vpc_id     = aws_vpc.main.id
  cidr_block = "10.0.14.0/24"

  tags = {
    Name = "private-e"
    Tier = "private"
  }
}
