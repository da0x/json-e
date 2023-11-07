from __future__ import absolute_import, print_function, unicode_literals

import math
import datetime
from nose.tools import eq_, assert_raises
from jsone.shared import string, stringDate
from jsone import render, JSONTemplateError


def test_custom_builtin():
    def my_builtin(x, y):
        return math.sqrt(x ** 2 + y ** 2)
    eq_(render({'$eval': 'my_builtin(3, 4)'}, {'my_builtin': my_builtin}), 5)

def test_non_object_context():
    assert_raises(JSONTemplateError, lambda: render({}, None))
    assert_raises(JSONTemplateError, lambda: render({}, False))
    assert_raises(JSONTemplateError, lambda: render({}, 3.2))
    assert_raises(JSONTemplateError, lambda: render({}, "two"))
    assert_raises(JSONTemplateError, lambda: render({}, [{}]))

def test_same_time_within_evaluation_operator():
    template = [{'$fromNow': ''} for _ in range(1000)]
    result = render(template, {})
    eq_(len(set(result)), 1)

def test_same_time_within_evaluation_builtin():
    template = [{'$eval': 'fromNow("")'} for _ in range(1000)]
    result = render(template, {})
    eq_(len(set(result)), 1)

def test_now_builtin():
    eq_(isinstance(render({'$eval': 'now'}, {}), string), True)

def test_stringDate_microseconds():
    eq_(stringDate(datetime.datetime(2017, 11, 1, 22, 0, 9, 0)), '2017-11-01T22:00:09.000Z')
