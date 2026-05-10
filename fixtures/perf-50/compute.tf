# 10 EC2 instances. 10 of 50 source resources.

resource "aws_instance" "web_1" {
  ami           = "ami-0abcd1234"
  instance_type = "t3.micro"

  tags = {
    Name = "web-1"
    Tier = "web"
  }
}

resource "aws_instance" "web_2" {
  ami           = "ami-0abcd1234"
  instance_type = "t3.micro"

  tags = {
    Name = "web-2"
    Tier = "web"
  }
}

resource "aws_instance" "web_3" {
  ami           = "ami-0abcd1234"
  instance_type = "t3.micro"

  tags = {
    Name = "web-3"
    Tier = "web"
  }
}

resource "aws_instance" "web_4" {
  ami           = "ami-0abcd1234"
  instance_type = "t3.micro"

  tags = {
    Name = "web-4"
    Tier = "web"
  }
}

resource "aws_instance" "web_5" {
  ami           = "ami-0abcd1234"
  instance_type = "t3.micro"

  tags = {
    Name = "web-5"
    Tier = "web"
  }
}

resource "aws_instance" "app_1" {
  ami           = "ami-0abcd1234"
  instance_type = "t3.small"

  tags = {
    Name = "app-1"
    Tier = "app"
  }
}

resource "aws_instance" "app_2" {
  ami           = "ami-0abcd1234"
  instance_type = "t3.small"

  tags = {
    Name = "app-2"
    Tier = "app"
  }
}

resource "aws_instance" "app_3" {
  ami           = "ami-0abcd1234"
  instance_type = "t3.small"

  tags = {
    Name = "app-3"
    Tier = "app"
  }
}

resource "aws_instance" "db_1" {
  ami           = "ami-0abcd1234"
  instance_type = "t3.medium"

  tags = {
    Name = "db-1"
    Tier = "db"
  }
}

resource "aws_instance" "db_2" {
  ami           = "ami-0abcd1234"
  instance_type = "t3.medium"

  tags = {
    Name = "db-2"
    Tier = "db"
  }
}
