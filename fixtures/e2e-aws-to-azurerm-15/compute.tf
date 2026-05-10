# 3 EC2 instances. 3 of 15 source resources.

resource "aws_instance" "web_1" {
  ami                    = "ami-0c55b159cbfafe1f0"
  instance_type          = "t3.micro"
  subnet_id              = aws_subnet.public_a.id
  vpc_security_group_ids = [aws_security_group.web.id]

  tags = {
    Name = "web-1"
    Tier = "web"
  }
}

resource "aws_instance" "web_2" {
  ami                    = "ami-0c55b159cbfafe1f0"
  instance_type          = "t3.micro"
  subnet_id              = aws_subnet.public_a.id
  vpc_security_group_ids = [aws_security_group.web.id]

  tags = {
    Name = "web-2"
    Tier = "web"
  }
}

resource "aws_instance" "app_1" {
  ami                    = "ami-0c55b159cbfafe1f0"
  instance_type          = "t3.small"
  subnet_id              = aws_subnet.private_a.id
  vpc_security_group_ids = [aws_security_group.app.id]

  tags = {
    Name = "app-1"
    Tier = "app"
  }
}
